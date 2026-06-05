---
name: docx
description: Use this skill whenever a Word document is the primary input or output. Trigger for reading, summarizing, extracting, creating, editing, converting, formatting, commenting, working with tracked changes, or any request that mentions a .doc, .docx, Word document, report, memo, letter, or template as a Word deliverable.
---

# Kivio DOCX Skill

## Kivio Runtime Notes

- Use the exact Kivio safe copy path shown in the attachment note. Do not infer document content from the filename.
- Kivio `run_python` is a Pyodide sandbox with no host filesystem access. Do not use it for Python examples that open the attached DOCX path.
- Do not install host Python packages with `pip` unless the user explicitly asks to modify their host Python environment.
- If a full imported DOCX skill provides bundled scripts under `scripts/`, run those with `skill_run_script`. Do not run skill scripts through `run_command`.
- When a command-line conversion creates text, markdown, PDF, or images, write temporary outputs under `/tmp`, then use `read_file` for text outputs.

## Reading Workflow

1. Activate this skill before processing a Word attachment.
2. For content extraction, prefer Pandoc when available:

```bash
pandoc --track-changes=all "$DOCX_PATH" -o /tmp/kivio_docx.md --wrap=none
```

Then call `read_file` on `/tmp/kivio_docx.md`.

3. If Pandoc is unavailable, use an installed office converter such as LibreOffice or platform tools if available, or inspect the DOCX ZIP/XML structure with standard tools.
4. Legacy `.doc` files usually need conversion to `.docx` or markdown before reliable analysis.

## Creation And Editing Workflow

- Preserve an existing document's structure and formatting unless the user asks for a redesign.
- For generated or edited Word files, validate by reopening or converting when an available tool supports it.
- Write outputs to a user-accessible path or a clearly named temporary path, then tell the user where the output is.
