# Ordinary Conversation Workbench Contract

## Scenario: Default workbench for non-project conversations

### 1. Scope / Trigger

Use this contract whenever native file/command tools, generated artifacts, conversation deletion, or settings migration need a default path for a chat that is not bound to a project.

The workbench is an ergonomic default, **not** a sandbox or permission boundary.

### 2. Signatures

Backend configuration:

```rust
pub struct ChatNativeToolsConfig {
    pub working_directory: String,
    // Deserialization compatibility only; sanitize clears it.
    pub workspace_roots: Vec<String>,
}
```

Runtime workspace:

```rust
pub struct NativeToolWorkspace {
    pub project: Option<ProjectWorkspaceContext>,
    pub default_directory: Option<PathBuf>,
}

pub fn conversation_workspace_directory(
    working_directory: &str,
    conversation_id: &str,
) -> Result<PathBuf, String>;
```

Frontend wire fields are camelCase:

```ts
type ChatNativeToolsConfig = {
  workingDirectory?: string
  workspaceRoots?: string[] // legacy import compatibility only
}
```

### 3. Contracts

- Default global root: `<home>/Kivio/workspace` on Windows and macOS.
- Ordinary chat directory: `<working_directory>/<conversation_id>`.
- Project chat directory: the bound project root; the ordinary root is ignored.
- Relative file paths and omitted command `cwd` resolve from the active workbench.
- Explicit absolute paths and `~/...` remain usable outside the workbench.
- The ordinary directory is created only when a relative/default-path operation or generated artifact needs it.
- `run_python` artifacts are written directly into the active workbench and must not add runtime metadata files such as `meta.json` to that directory.
- Ordinary-chat writes inside the workbench may become downloadable artifact cards; project edits and explicit external writes do not automatically become cards.
- `Kivio/outputs/<conversation_id>` is legacy input only. Runtime code must not create new output directories there.
- Deleting an ordinary conversation deletes only the directory derived from the configured root plus a validated conversation ID. Project roots are never deleted.

### 4. Validation & Error Matrix

| Condition | Required result |
|---|---|
| Conversation ID is not `conv_` plus ASCII alphanumeric/`_`/`-` | Reject before constructing, migrating, or deleting a workbench path |
| Configured root cannot be created | Return an explicit tool error; do not silently fall back |
| Migration target contains a same-name file/type conflict | Abort without overwrite and retain source data |
| Conversation has a non-empty `project_id`, even if the project record is stale | Treat as project-bound for migration/deletion safety |
| Conversation file is missing but index metadata exists | Use index metadata to decide project safety and remove the stale index entry |
| Both conversation file and metadata are missing | Do not guess that a workbench is safe to delete |
| Explicit absolute path is used | Resolve that path without creating an unrelated ordinary workbench |

### 5. Good / Base / Bad Cases

- **Good:** `write("report.pptx")` in an ordinary chat writes to `<root>/<conversation>/report.pptx` and creates the directory on demand.
- **Good:** the user requests `C:\Users\me\Desktop\report.pptx`; the explicit path is honored and the ordinary directory is not created just for that write.
- **Base:** a project chat uses its project root for relative paths and generated artifacts.
- **Bad:** treating `working_directory` as an allow-list and rejecting paths outside it.
- **Bad:** writing artifact bookkeeping files into a project root or user workbench.
- **Bad:** moving or deleting a directory supplied directly by a caller instead of deriving it from a validated conversation ID.

### 6. Tests Required

- Settings sanitize migrates `workspace_roots[0]`, clears legacy roots, and falls back to the platform default.
- Relative read/write and omitted command `cwd` create/use the ordinary workbench.
- Explicit absolute paths do not create the ordinary workbench.
- Project-relative paths remain rooted at the project directory.
- Invalid/path-like conversation IDs are rejected.
- Legacy outputs and root changes merge without overwrite; conflicts leave source files intact.
- Artifact path rewriting covers both `message.artifacts` and `message.tool_calls[*].artifacts`.
- More than 16 existing workbench files are not pruned.
- `run_python` exports into the supplied directory and does not create internal `meta.json` bookkeeping.
- Generated-file open/reveal accepts any existing absolute artifact path.

### 7. Wrong vs Correct

#### Wrong

```rust
// Conflates a default workbench with a security boundary.
if !resolved.starts_with(&working_root) {
    return Err("path outside workspace".into());
}
```

#### Correct

```rust
let resolved = if path.is_absolute() {
    canonicalize_existing_or_missing(path, allow_missing)?
} else {
    canonicalize_existing_or_missing(&workbench.join(path), allow_missing)?
};
```

The workbench chooses the default location only; execution approval and filesystem permissions remain separate concerns.
