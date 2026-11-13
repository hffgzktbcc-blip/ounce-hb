#!/usr/bin/env python3
"""Post the missing-details triage comment on a newly opened issue.

Called by .github/workflows/issue-triage.yml. It never closes, locks or labels anything: items that
match the scope policy are left to .github/workflows/policy.yml (see triage_policy.py), so this
script skips them rather than adding a second comment.

Environment: GH_REPO, ISSUE_NUMBER, ISSUE_TITLE, ISSUE_BODY, ISSUE_AUTHOR, GH_TOKEN.
"""
import json
import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Tuple

sys.path.insert(0, str(Path(__file__).resolve().parent))

from gh_client import Runner, run_gh  # noqa: E402
from triage_policy import classify  # noqa: E402

TRIAGE_MARKER = "<!-- ounce-hb-issue-triage-v1 -->"
NO_RESPONSE = "_no response_"

# Form headings from .github/ISSUE_TEMPLATE, matched as whole words.
PROGRAM_HEADING = re.compile(r"\b(?:program|homebrew|source|repository)\b")
VERSION_HEADING = re.compile(r"\b(?:version|commit|release|tag)\b")
COMMAND_HEADING = re.compile(r"\b(?:command|repro|reproduce|reproduction|steps?)\b")
OUTPUT_HEADING = re.compile(
    r"\b(?:output|log|logs|behaviou?r|expected|observed|happened|expect|error)\b"
)

# Free-text signals. Each must be specific: a bare "app" or "version" is not evidence.
PROGRAM_TEXT = re.compile(r"\b[\w.-]+\.(?:nro|nso|elf)\b|https?://github\.com/[\w.-]+/[\w.-]+", re.I)
VERSION_TEXT = re.compile(
    r"\bv\d+\.\d+(?:\.\d+)?\b"
    r"|\bcommit\s+[0-9a-f]{7,40}\b"
    r"|\bversion\s*[:=]?\s*v?\d"
    r"|\b(?=[0-9a-f]*\d)(?=[0-9a-f]*[a-f])[0-9a-f]{7,40}\b",
    re.I,
)
COMMAND_TEXT = re.compile(
    r"\bcargo\s+run\b|\bounce-hb\s+run\b|\bounce-hb\s+--|\./ounce-hb\b|^```(?:sh|shell|bash|console)\b",
    re.I | re.M,
)
OUTPUT_TEXT = re.compile(
    r"\bexit\s+(?:code|status)\s*[:=]?\s*-?\d+\b|\bpanicked\b|\berror:|\bstderr\b|\bstdout\b|\boutput:",
    re.I,
)

MISSING_TEXT = {
    "program": "**Homebrew Program**: The name or link to the homebrew NRO/NSO built with devkitPro/libnx.",
    "version": "**Version**: The commit hash or release tag of ounce-hb.",
    "command": "**Command**: The exact command line used (for example, `cargo run -- run path/to/app.nro`).",
    "output": "**Output**: The console output or error log observed.",
}


def parse_sections(body: str) -> Dict[str, str]:
    """Split an issue-form body on its `### Heading` lines. Keys are lowercased headings."""
    sections: Dict[str, str] = {}
    heading: Optional[str] = None
    content: List[str] = []
    for line in (body or "").splitlines():
        m = re.match(r"^###\s+(.+)$", line.strip())
        if m:
            if heading is not None:
                sections[heading] = "\n".join(content).strip()
            heading = m.group(1).strip().lower()
            content = []
        elif heading is not None:
            content.append(line)
    if heading is not None:
        sections[heading] = "\n".join(content).strip()
    return sections


def _filled(content: str) -> bool:
    text = content.strip()
    return bool(text) and text.lower() != NO_RESPONSE


def _section_signal(sections: Dict[str, str], heading: re.Pattern) -> bool:
    return any(_filled(c) for h, c in sections.items() if heading.search(h))


def missing_details(title: str, body: str) -> List[str]:
    """Return the keys ('program', 'version', 'command', 'output') with no real evidence."""
    sections = parse_sections(body)
    text = f"{title or ''}\n{body or ''}"
    present = {
        "program": _section_signal(sections, PROGRAM_HEADING) or bool(PROGRAM_TEXT.search(text)),
        "version": _section_signal(sections, VERSION_HEADING) or bool(VERSION_TEXT.search(text)),
        "command": _section_signal(sections, COMMAND_HEADING) or bool(COMMAND_TEXT.search(text)),
        "output": _section_signal(sections, OUTPUT_HEADING) or bool(OUTPUT_TEXT.search(text)),
    }
    return [key for key in ("program", "version", "command", "output") if not present[key]]


def find_duplicates(
    open_items: Sequence[dict], title: str, issue_number: str, limit: int = 3
) -> List[Tuple[object, str]]:
    """Open issues (not PRs) that share at least two words of 4+ characters with the title."""
    title_words = set(re.findall(r"\b[a-z0-9_-]{4,}\b", (title or "").lower()))
    found: List[Tuple[object, str]] = []
    for item in open_items:
        if str(item.get("number")) == str(issue_number) or item.get("pull_request") is not None:
            continue
        item_words = set(re.findall(r"\b[a-z0-9_-]{4,}\b", (item.get("title") or "").lower()))
        if len(title_words & item_words) >= 2:
            found.append((item.get("number"), item.get("title")))
    return found[:limit]


def build_comment(author: str, missing: List[str], duplicates: List[Tuple[object, str]]) -> str:
    lines = [TRIAGE_MARKER, f"Thank you @{author} for opening this issue.", ""]
    if missing:
        lines.append("### Missing reproduction details")
        lines.append(
            "To help verify and investigate this homebrew report, please provide the following missing details:"
        )
        lines += [f"- {MISSING_TEXT[key]}" for key in missing]
        lines.append("")
    else:
        lines.append("All expected reproduction details (program, version, command, output) appear to be provided.")
        lines.append("")
    if duplicates:
        lines.append("### Potential related open issues")
        # Numbers only: other items' titles are untrusted text, so they are not echoed.
        lines += [f"- #{num}" for num, _title in duplicates]
        lines.append("")
    return "\n".join(lines).strip()


def main(env: Optional[Dict[str, str]] = None, runner: Optional[Runner] = None) -> int:
    env = dict(os.environ if env is None else env)
    runner = runner or run_gh
    repo = env.get("GH_REPO", "")
    number = env.get("ISSUE_NUMBER", "")
    title = env.get("ISSUE_TITLE", "")
    body = env.get("ISSUE_BODY", "")
    author = env.get("ISSUE_AUTHOR", "")
    if not repo or not number:
        print("Missing issue information; nothing to do.")
        return 0
    if classify(title, body).action == "flag":
        print(f"#{number} matches the scope policy; policy.yml handles it, no triage comment.")
        return 0

    duplicates: List[Tuple[object, str]] = []
    try:
        out = runner(["api", f"repos/{repo}/issues?state=open&per_page=30"])
        duplicates = find_duplicates(json.loads(out), title, number)
    except Exception as e:  # a failed duplicate search must not block the triage comment
        print(f"Warning: failed to query open issues: {e}", file=sys.stderr)

    comment = build_comment(author, missing_details(title, body), duplicates)
    try:
        runner(["api", "-X", "POST", f"repos/{repo}/issues/{number}/comments", "-f", f"body={comment}"])
    except Exception as e:
        print(f"Failed to post triage comment: {e}", file=sys.stderr)
        return 1
    print(f"Posted triage comment on #{number}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
