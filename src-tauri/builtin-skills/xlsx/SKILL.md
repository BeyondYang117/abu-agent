---
name: xlsx
description: Use this skill whenever a spreadsheet file is the primary input or output. Trigger for reading, summarizing, cleaning, editing, fixing, formatting, charting, recalculating, creating, or converting .xlsx, .xls, .xlsm, .csv, or .tsv files.
---

# Kivio XLSX Skill

## Kivio Runtime Notes

- Use the exact Kivio safe copy path shown in the attachment note. Do not infer spreadsheet content from the filename.
- Kivio `run_python` is a Pyodide sandbox with no host filesystem access. Do not use it for Python examples that open the attached XLSX path.
- Do not install host Python packages with `pip` unless the user explicitly asks to modify their host Python environment.
- If a full imported spreadsheet skill provides bundled scripts under `scripts/`, run those with `skill_run_script`. Do not run skill scripts through `run_command`.
- When a command-line conversion creates text, markdown, CSV, or JSON, write temporary outputs under `/tmp`, then use `read_file` for text outputs.

## Reading Workflow

1. Activate this skill before processing a spreadsheet attachment.
2. For `.csv` and `.tsv`, use `read_file` directly on the Kivio safe copy path when the file is text-sized enough to inspect.
3. For `.xlsx`, `.xls`, and `.xlsm`, prefer an installed converter or spreadsheet tool:

```bash
mkdir -p /tmp/kivio_xlsx
soffice --headless --convert-to csv --outdir /tmp/kivio_xlsx "$XLSX_PATH"
```

Then call `read_file` on the generated CSV. If LibreOffice is unavailable, use another installed tool or an already-available host Python library, but do not install packages automatically.

4. For multi-sheet workbooks, identify sheet names first when possible, then extract the relevant sheet(s) instead of assuming the first sheet is enough.

## Creation And Editing Workflow

- Preserve existing workbook structure, formulas, and formatting unless the user asks for a redesign.
- Prefer formulas over hardcoded calculated values when producing spreadsheets.
- Recalculate and check for formula errors when an available tool supports it.
- Write outputs to a user-accessible path or a clearly named temporary path, then tell the user where the output is.
