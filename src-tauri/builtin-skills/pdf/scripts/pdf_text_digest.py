#!/usr/bin/env python3
"""Create a compact reading digest from text extracted with pdftotext."""

from __future__ import annotations

import argparse
import re
from pathlib import Path


SECTION_PATTERNS = [
    ("abstract", r"(?im)^\s*(?:abstract|摘要|抽象的?)"),
    ("introduction", r"(?im)^\s*(?:1\s+introduction|introduction|一[、.]\s*(?:简介|引言)|1[.、 ]+\s*(?:简介|引言))"),
    ("literature review", r"(?im)^\s*(?:2(?:\.\d+)?[.、 ]*\s*(?:文献|综述|literature|review))"),
    ("problem/model", r"(?im)^\s*(?:3(?:\.\d+)?[.、 ]*\s*(?:问题|模型|表述|假设|符号|problem|model|mathematical|formulation))"),
    ("solution method", r"(?im)^\s*(?:4(?:\.\d+)?[.、 ]*\s*(?:解决|算法|方法|solution|algorithm|method|proposed))"),
    ("experiments/results", r"(?im)^\s*(?:5(?:\.\d+)?[.、 ]*\s*(?:数据|实验|结果|案例|真实案例|computational|experiment|result|case))"),
    ("management/sensitivity", r"(?im)^\s*(?:5(?:\.\d+)?[.、 ]*\s*(?:管理|敏感性|建议|应用|sensitivity|managerial|management))"),
    ("conclusion", r"(?im)(?:^\s*(?:6[.、 ]*\s*)?(?:结论|结语|conclusion|conclusions)|^.{0,80}6[.、 ]*\s*结论)"),
]


def normalize_text(text: str) -> str:
    text = text.replace("\r\n", "\n").replace("\r", "\n").replace("\f", "\n\n")
    text = re.sub(r"[ \t]+", " ", text)
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text.strip()


def line_offsets(text: str) -> list[int]:
    offsets = [0]
    for match in re.finditer("\n", text):
        offsets.append(match.end())
    return offsets


def line_number_for_offset(offsets: list[int], char_offset: int) -> int:
    low = 0
    high = len(offsets)
    while low + 1 < high:
        mid = (low + high) // 2
        if offsets[mid] <= char_offset:
            low = mid
        else:
            high = mid
    return low + 1


def snippet_at(text: str, start: int, max_chars: int, stop: int | None = None) -> str:
    end = min(len(text), start + max_chars, stop if stop is not None else len(text))
    snippet = text[start:end]
    if len(snippet) == max_chars:
        last_boundary = max(snippet.rfind("\n\n"), snippet.rfind(". "), snippet.rfind("。"))
        if last_boundary > max_chars // 2:
            snippet = snippet[: last_boundary + 1]
    return snippet.strip()


def find_sections(text: str, max_chars: int) -> list[tuple[str, int, str]]:
    offsets = line_offsets(text)
    matches: list[tuple[str, int]] = []
    used_starts: set[int] = set()
    for label, pattern in SECTION_PATTERNS:
        match = re.search(pattern, text)
        if not match:
            continue
        start = match.start()
        if start in used_starts:
            continue
        used_starts.add(start)
        matches.append((label, start))
    matches.sort(key=lambda item: item[1])

    sections: list[tuple[str, int, str]] = []
    for index, (label, start) in enumerate(matches):
        next_start = matches[index + 1][1] if index + 1 < len(matches) else None
        sections.append((
            label,
            line_number_for_offset(offsets, start),
            snippet_at(text, start, max_chars, next_start),
        ))
    return sections


def head_tail_digest(text: str, max_chars: int) -> list[tuple[str, int, str]]:
    offsets = line_offsets(text)
    chunks = [("opening", 0), ("ending", max(0, len(text) - max_chars))]
    return [
        (label, line_number_for_offset(offsets, start), snippet_at(text, start, max_chars))
        for label, start in chunks
    ]


def build_digest(text: str, source: Path, max_chars_per_section: int) -> str:
    text = normalize_text(text)
    sections = find_sections(text, max_chars_per_section)
    if len(sections) < 3:
        sections = head_tail_digest(text, max_chars_per_section)

    lines = [
        "# PDF reading digest",
        "",
        f"Source text: `{source}`",
        f"Total characters: {len(text)}",
        "",
        "Use this digest for summary-style questions before reading the full extracted text.",
        "If the user asks for exact evidence, formulas, tables, or a specific section, read the source text around the cited line.",
        "",
    ]
    for label, line_number, snippet in sections:
        lines.extend(
            [
                f"## {label} (around line {line_number})",
                "",
                snippet,
                "",
            ]
        )
    return "\n".join(lines).strip() + "\n"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("input", help="Text file produced by pdftotext")
    parser.add_argument("output", help="Markdown digest output path")
    parser.add_argument("--max-chars-per-section", type=int, default=1500)
    args = parser.parse_args()

    source = Path(args.input)
    output = Path(args.output)
    text = source.read_text(encoding="utf-8", errors="replace")
    digest = build_digest(text, source, max(800, args.max_chars_per_section))
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(digest, encoding="utf-8")
    print(f"Wrote digest to {output} ({len(digest)} chars)")


if __name__ == "__main__":
    main()
