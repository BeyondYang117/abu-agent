# MCP Tool Discovery State

> Applies to `src-tauri/src/mcp/registry.rs`, MCP session management, startup warmup, and the headless `src-tauri/src/kivio_code/mcp_setup.rs` path.

## Single discovery path

- Do not keep an aggregate chat-tool cache. Each request assembles tools from the current settings and current plugin runtime eligibility.
- GUI chat and `kivio-code` must use the same MCP discovery function, per-tool `enabled_tools` setting, and runtime eligibility predicate.
- Do not add outer timeouts or fallback layers around the session manager. The manager owns RPC timeouts, reconnect single-flight, and reconnect backoff.
- Native tools, Skill definitions, and MCP tools are combined only after MCP discovery; Skill activation must never remove an enabled tool.

## Authoritative states

There are only two MCP schema states:

1. The live session schema retained by `McpSession` after a successful paginated `tools/list`.
2. The persisted last-known schema snapshot for the exact same server configuration fingerprint.

No third aggregate schema/cache is allowed. A running MCP process is not proof that a tool was sent to the model; the authoritative evidence is the tool-definition array recorded on the actual model request.

## Failure behavior

For every currently eligible server:

- Live discovery succeeds: use the live schema and replace the matching last-known snapshot.
- Live discovery fails and a snapshot with the same configuration fingerprint exists: expose that last-known schema so an explicitly enabled tool does not disappear because of a temporary disconnect.
- Live discovery fails and no matching snapshot exists: expose no tools for that server and record it as unavailable for the request prompt.

One failed server must not block native tools or healthy MCP servers. Explicit tool execution bypasses discovery cooldown and attempts reconnection immediately.

## Configuration and eligibility

- Plain MCP servers are eligible only when enabled in settings.
- Plugin-backed MCP servers are eligible only when their settings entry is enabled and the owning plugin is installed and enabled.
- `enabled_tools` is the only per-tool MCP allow-list. An empty list means all advertised tools are enabled.
- A configuration fingerprint must cover the connection configuration without persisting raw credentials. A changed fingerprint must never reuse the previous schema snapshot.
- Plugin enable/disable and configuration edits take effect on the next discovery naturally; no cache invalidation hook is required.

## Startup and lifecycle

- Startup warmup is only an optimization that pre-connects eligible servers. It is not part of tool-list correctness and must not populate or invalidate an aggregate cache.
- Idle reap, reload, disconnect, and process restart may discard live sessions without discarding a matching last-known schema.
- Reconnect attempts use the session manager's bounded backoff so an unavailable server does not impose a full handshake timeout on every request.

## Dynamic schemas

- Stdio `notifications/tools/list_changed` increments the connection schema revision.
- The next discovery must refresh all `tools/list` pages and replace the live and last-known schemas.
- Streamable HTTP push notifications remain unsupported until a persistent server-event listener exists.

## Required regression

Tests and live validation must demonstrate:

1. GUI and headless discovery apply the same runtime eligibility and `enabled_tools` rules.
2. Temporary discovery failure uses only a matching last-known schema.
3. A server with no matching snapshot is reported unavailable without hiding native or healthy-server tools.
4. The first model request after startup contains every expected `mcp__<server>__<tool>` definition.
5. Skill activation does not change the enabled tool-definition set.
