#!/usr/bin/env python3
"""Extract searchable PDF text quietly and print a compact reading digest."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

from pdf_text_digest import build_digest


def run_pdftotext(pdf_path: Path, text_output: Path) -> None:
    pdftotext = shutil.which("pdftotext")
    if not pdftotext:
        raise RuntimeError("pdftotext is not available on this machine")

    text_output.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        [pdftotext, "-q", "-layout", str(pdf_path), str(text_output)],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout or "").strip()
        raise RuntimeError(detail or f"pdftotext failed with exit code {result.returncode}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("pdf", help="PDF path shown in the Kivio attachment note")
    parser.add_argument("--text-output", default="/tmp/kivio_pdf_text.txt")
    parser.add_argument("--digest-output", default="/tmp/kivio_pdf_digest.md")
    parser.add_argument("--max-chars-per-section", type=int, default=1500)
    args = parser.parse_args()

    pdf_path = Path(args.pdf)
    if not pdf_path.is_file():
        raise SystemExit(f"PDF file not found: {pdf_path}")

    text_output = Path(args.text_output)
    digest_output = Path(args.digest_output)
    run_pdftotext(pdf_path, text_output)

    text = text_output.read_text(encoding="utf-8", errors="replace")
    if not text.strip():
        digest = (
            "# PDF reading digest\n\n"
            f"Source PDF: `{pdf_path}`\n"
            f"Source text: `{text_output}`\n\n"
            "No searchable text was extracted. This PDF may need OCR or visual review.\n"
        )
    else:
        digest = build_digest(text, text_output, max(800, args.max_chars_per_section))

    digest_output.parent.mkdir(parents=True, exist_ok=True)
    digest_output.write_text(digest, encoding="utf-8")
    sys.stdout.write(digest)


if __name__ == "__main__":
    main()
