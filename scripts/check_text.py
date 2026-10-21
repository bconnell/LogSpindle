#!/usr/bin/env python3
"""Validate repository text before local Rust verification or publication."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parent.parent
SKIP_DIRS = {".git", "target", ".idea", ".vscode", "__pycache__"}
TEXT_EXTENSIONS = {".md", ".toml", ".rs", ".py", ".sh", ".ps1", ".jsonl"}
REQUIRED = {
    "README.md",
    "Cargo.toml",
    "docs/VALIDATION.md",
    "src/main.rs",
    "tests/cli.rs",
    "tests/fixtures/sample.jsonl",
    "scripts/check_text.py",
    "scripts/verify.ps1",
    "scripts/verify.sh",
}
PLACEHOLDER = re.compile(
    r"\b(?:TODO|TBD|FIXME|lorem ipsum|coming soon|production.ready|guaranteed)\b",
    re.IGNORECASE,
)
SECRETS = [
    re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    re.compile(r"\b(?:ghp_|github_pat_)[A-Za-z0-9_]{20,}"),
    re.compile(
        r"""(?i)\b(?:api[_-]?key|password|secret)\s*[:=]\s*["'][^"']{8,}["']"""
    ),
]
LINKS = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
HEADINGS = re.compile(r"^#{1,6}\s+(.+)$", re.MULTILINE)


def slug(value: str) -> str:
    value = value.strip().lower()
    value = re.sub(r"[^a-z0-9\s-]", "", value)
    return re.sub(r"\s+", "-", value)


def check_links(path: Path, content: str) -> list[str]:
    errors = []
    for match in LINKS.finditer(content):
        target = match.group(1).strip().split(" ", 1)[0]
        if target.startswith(("http:", "https:", "mailto:", "data:")):
            continue
        base, _, fragment = target.partition("#")
        referenced = (path.parent / unquote(base)).resolve() if base else path
        if not referenced.is_relative_to(ROOT):
            errors.append(f"{path.relative_to(ROOT)}: link escapes repository: {target}")
            continue
        if not referenced.exists():
            errors.append(f"{path.relative_to(ROOT)}: missing link: {target}")
            continue
        if fragment and referenced.suffix.lower() == ".md":
            heading_text = referenced.read_text(encoding="utf-8")
            anchors = {slug(title) for title in HEADINGS.findall(heading_text)}
            if unquote(fragment) not in anchors:
                errors.append(
                    f"{path.relative_to(ROOT)}: missing section anchor: {target}"
                )
    return errors


def main() -> int:
    errors: list[str] = []
    for name in sorted(REQUIRED):
        if not (ROOT / name).is_file():
            errors.append(f"missing required file: {name}")

    checked = 0
    for path in sorted(ROOT.rglob("*")):
        if not path.is_file() or any(part in SKIP_DIRS for part in path.relative_to(ROOT).parts):
            continue
        if path.suffix not in TEXT_EXTENSIONS and path.name != ".gitignore":
            continue
        relative = path.relative_to(ROOT)
        checked += 1
        try:
            data = path.read_bytes()
            content = data.decode("utf-8")
        except UnicodeDecodeError:
            errors.append(f"{relative}: text must be UTF-8")
            continue

        if data.startswith(b"\xef\xbb\xbf"):
            errors.append(f"{relative}: unexpected UTF-8 BOM")
        if any(char in content for char in ("\x00", "\x1b", "\x7f")):
            errors.append(f"{relative}: forbidden control character")
        for number, line in enumerate(content.splitlines(), 1):
            if line.rstrip(" \t") != line:
                errors.append(f"{relative}:{number}: trailing whitespace")
            if path.suffix == ".md" and len(line) > 140:
                errors.append(f"{relative}:{number}: documentation line exceeds 140 characters")

        for pattern in SECRETS:
            if pattern.search(content):
                errors.append(f"{relative}: possible credential or private key material")

        if path.suffix == ".md":
            if PLACEHOLDER.search(content):
                errors.append(f"{relative}: unsupported placeholder or promotional claim")
            if re.search(r"(?i)github actions? (?:outage|unavailable|disabled)", content):
                errors.append(f"{relative}: internal infrastructure status leaked")
            if re.search(r"(?i)(?:C:\\\\Users\\\\|/home/[^/\s]+/)", content):
                errors.append(f"{relative}: personal file path leaked")
            errors.extend(check_links(path, content))

    readme = ROOT / "README.md"
    if readme.is_file():
        text = readme.read_text(encoding="utf-8")
        for section in ("## Input format", "## Usage", "## Limitations", "## Validation"):
            if section not in text:
                errors.append(f"README.md: missing section: {section}")

    for problem in errors:
        print(f"FAIL: {problem}", file=sys.stderr)
    if errors:
        print(f"Text gate failed: {len(errors)} issue(s)", file=sys.stderr)
        return 1

    print(f"Text gate passed: {checked} files inspected; required docs, links, and content checked.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
