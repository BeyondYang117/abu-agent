# Tauri / Native Platform Specs

> **Purpose**: Contracts and conventions for native window management and platform process/startup concerns (Tauri windows, platform window styling, overlay lifecycle, process environment wiring).

---

## Available Specs

| Spec | Purpose | When to Use |
|------|---------|-------------|
| [Overlay Window Contracts](./overlay-window-contracts.md) | Floating-overlay vs application-window archetypes; Windows transparency fragility rules | Touching `windows.rs` / `lens_commands.rs` window config, or fixing overlay rendering bugs on Windows |
| [Process PATH Enrichment](./process-path-enrichment.md) | Startup PATH fixup so subprocesses find user CLIs; two-phase Windows order + profile probe for fnm/nvm; `-NoProfile` invariant | Touching `path_env.rs` or its `lib.rs` call site; debugging "GUI can't find node/CLI" or version-manager (fnm/nvm) PATH issues |

---

**Language**: All documentation is written in **English**.
