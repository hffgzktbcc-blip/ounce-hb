#!/usr/bin/env python3
"""Publish docs/wiki/*.md to the GitHub wiki, rewriting repo-relative links.

docs/wiki/ is the source of truth. The wiki git repo must already exist (GitHub creates it when a
first page is saved in the browser). Usage:

    python3 -I scripts/publish_wiki.py --check            # print the rewritten pages, change nothing
    python3 -I scripts/publish_wiki.py --repo OWNER/NAME  # clone the wiki, update, commit, push

Link rewriting: `[x](FAQ.md)` -> `[x](FAQ)` for wiki pages; `../../FILE.md` -> blob/main/FILE.md;
`../FILE.md` -> blob/main/docs/FILE.md. Absolute http(s) links are left alone.
"""
import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "docs" / "wiki"


def rewrite(text: str, pages: set, blob: str) -> str:
    def repl(m):
        label, target = m.group(1), m.group(2)
        if re.match(r"https?://|#|mailto:", target):
            return m.group(0)
        path, _, frag = target.partition("#")
        frag = "#" + frag if frag else ""
        if path.startswith("../../"):
            return f"[{label}]({blob}/{path[6:]}{frag})"
        if path.startswith("../"):
            return f"[{label}]({blob}/docs/{path[3:]}{frag})"
        name = path[:-3] if path.endswith(".md") else path
        if name in pages:
            return f"[{label}]({name}{frag})"
        return m.group(0)

    return re.sub(r"\[([^\]]*)\]\(([^)\s]+)\)", repl, text)


def build(repo: str) -> dict:
    blob = f"https://github.com/{repo}/blob/main"
    pages = {p.stem for p in SRC.glob("*.md")}
    return {p.name: rewrite(p.read_text(), pages, blob) for p in sorted(SRC.glob("*.md"))}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", "hffgzktbcc-blip/ounce-hb"))
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()
    out = build(args.repo)
    if not out:
        print("no pages in docs/wiki", file=sys.stderr)
        return 1
    if args.check:
        for name, text in out.items():
            print(f"--- {name} ({len(text)} bytes)")
        return 0
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    url = f"https://github.com/{args.repo}.wiki.git"
    if token:
        url = f"https://x-access-token:{token}@github.com/{args.repo}.wiki.git"
    with tempfile.TemporaryDirectory() as d:
        git = lambda *a: subprocess.run(["git", "-C", d, *a], check=True, text=True, capture_output=True)
        subprocess.run(["git", "clone", "-q", url, d], check=True, capture_output=True)
        for name, text in out.items():
            (Path(d) / name).write_text(text)
        git("add", "-A")
        if not git("status", "--porcelain").stdout.strip():
            print("wiki already up to date")
            return 0
        git("-c", "user.name=github-actions[bot]", "-c", "user.email=41898282+github-actions[bot]@users.noreply.github.com",
            "commit", "-qm", "Publish wiki pages from docs/wiki")
        git("push", "-q", "origin", "HEAD")
        print("wiki updated")
    return 0


if __name__ == "__main__":
    sys.exit(main())
