---
id: pdf
name: pdf
description: Read, extract, summarize, and analyze PDF attachments saved by Kivio Chat.
recommended-tools:
  - read_file
  - run_python
---

# PDF Skill

Use this skill when the user attaches or references a PDF and asks to read, summarize, extract, compare, translate, inspect, or answer questions about it.

## Inputs

Kivio stores each uploaded file as a safe local copy and includes its absolute path in the user message under `Kivio 安全副本路径`. Work on that safe copy path, not on the original user file.

## Workflow

1. Identify the PDF path from the attachment note.
2. Use `run_python` to inspect or extract text from the PDF when possible.
3. If extraction fails or returns little text, explain that the PDF may be scanned/image-only and ask for OCR or use the screenshot/Lens flow when appropriate.
4. Ground answers in extracted content. Do not infer document contents from the filename alone.
5. For long PDFs, extract page-level text first, then summarize by section/page before answering.

## Python Pattern

Use Pyodide-friendly Python. Prefer lightweight standard-library inspection first. If a PDF parser is unavailable in the sandbox, report that limitation clearly and ask the user whether to use another route.

```python
from pathlib import Path

pdf_path = Path("PASTE_SAFE_COPY_PATH_HERE")
print(pdf_path.name, pdf_path.stat().st_size)
```

## Output

- For summaries: include the main points and mention any pages/sections you used when available.
- For extraction: preserve original order and tables/lists as much as practical.
- For analysis: quote only short snippets and explain conclusions separately.
