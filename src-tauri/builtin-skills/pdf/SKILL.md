---
name: pdf
description: Use this skill whenever a PDF file is the primary input or output. Trigger for reading, summarizing, extracting text or tables, checking metadata, splitting, merging, rotating, watermarking, filling forms, converting, OCR-related PDF work, or any user request that mentions a .pdf attachment or path.
---

# Kivio PDF Skill

## Kivio Runtime Notes

- Use the exact Kivio safe copy path shown in the attachment note. Do not infer PDF content from the filename.
- Kivio `run_python` is a Pyodide sandbox with no host filesystem access. Do not use it for Python examples that open the attached PDF path.
- Do not install host Python packages with `pip` unless the user explicitly asks to modify their host Python environment.
- Use `skill_run_script` for scripts bundled inside this skill's `scripts/` directory.
- When a command-line extraction creates text, markdown, CSV, or images, write temporary outputs under `/tmp`, then use `read_file` for text outputs.

## Reading Workflow

1. Activate this skill before processing a PDF attachment.
2. For summary-style requests, create the compact digest in one step:

```text
skill_run_script(
  name="pdf",
  relative_path="scripts/pdf_extract_digest.py",
  args=["$PDF_PATH"]
)
```

The script runs `pdftotext -q -layout`, writes `/tmp/kivio_pdf_text.txt` and `/tmp/kivio_pdf_digest.md`, and returns the digest directly. Do not call `read_file` on the digest after this unless the script output was truncated.

Only read `/tmp/kivio_pdf_text.txt` directly when the user asks for exact evidence, formulas, tables, or a specific section. If direct reading is needed, use targeted `offset`/`limit` around the relevant section instead of paging through the whole file.

3. For page counts or metadata, prefer command-line tools when available:

```bash
pdfinfo "$PDF_PATH"
```

4. If `pdftotext` or `pdfinfo` is unavailable, use another installed command-line PDF tool if present. Do not retry the same missing tool or package import repeatedly.
5. For scanned PDFs where text extraction is empty, say that OCR is needed and use available OCR or vision workflows only if the enabled tools support them.

## Modification Workflow

- For simple page operations, use installed tools such as `qpdf` when available.
- Always write generated PDFs to a user-accessible path or a clearly named temporary path, then tell the user where the output is.
- Preserve the source PDF unless the user explicitly asks to overwrite it.
