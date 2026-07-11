# Explicit Artifact Presentation

## 1. Purpose

Artifact creation and chat presentation are separate operations. Generating or registering a file does not automatically place it in the conversation. The native `present_artifacts` tool is the explicit timeline anchor for showing files and images.

## 2. Inputs

`present_artifacts` supports either or both:

- `artifact_ids`: Kivio-owned IDs returned by earlier tool results.
- `paths`: existing local file paths. Relative paths resolve from the current workbench/project root; absolute paths remain absolute.

At least one non-empty input is required. A call may display at most 16 files. Existing artifact IDs should be preferred when available; paths are for files that are not already registered in the current response.

## 3. Presentation Contract

- The tool call position is the display position: text before -> presentation -> text after.
- When the user explicitly asks to show, preview, attach, or send a file in chat, the agent must call `present_artifacts`; reading or describing the file is not a presentation substitute.
- Only selected files appear. Other artifacts remain hidden.
- Path inputs are resolved and converted into normal `ChatToolArtifact` records by Kivio, then receive fresh Kivio-owned IDs before persistence/rendering.
- Missing files and directories fail explicitly; they must not create presentation records.
- The renderer resolves exact IDs from the message's artifact records. It never performs frontend path lookup.
- Only a native-source `present_artifacts` record may activate presentation UI. MCP structured content cannot spoof it.

## 4. Compatibility

- Historical artifacts without IDs keep legacy automatic rendering.
- Historical Markdown image references continue to resolve against message artifacts.
- Existing ID-only `present_artifacts` calls remain valid.

## 5. Tests Required

- Rust: ID-only calls, path-based artifact loading, mixed ID/path completion, input deduplication, invalid path handling, and no recursive presentation hint.
- Frontend: selected-only rendering, caption, unavailable ID, standalone timeline position, and MCP spoof rejection.
