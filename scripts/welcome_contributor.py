#!/usr/bin/env python3
"""Post a welcome comment on the first issue or pull request from a new contributor.

Called by .github/workflows/welcome.yml. The author's prior items are looked up through the API,
and no item text is read or executed. Pull requests are judged by metadata only.

Environment: GH_REPO, ISSUE_NUMBER, AUTHOR_LOGIN, REPO_OWNER, IS_PR ('true'|'false'), GH_TOKEN.
"""
import json
import os
import sys
from pathlib import Path
from typing import Dict, List, Optional

sys.path.insert(0, str(Path(__file__).resolve().parent))

from gh_client import Runner, run_gh  # noqa: E402

WELCOME_MARKER = "<!-- ounce-hb-welcome-v1 -->"


def should_skip_author(author: str, owner: str) -> bool:
    return author.lower() == owner.lower() or author.lower().endswith("[bot]")


def build_welcome(repo: str, item_type: str) -> str:
    lines = [
        WELCOME_MARKER,
        f"Thank you for opening your first {item_type} in ounce-hb.",
        "",
        f"A maintainer will review your {item_type}. In the meantime, you can run local checks with:",
        "```sh",
        "cargo fmt --all -- --check",
        "cargo clippy --workspace --all-targets -- -D warnings",
        "cargo test --workspace",
        "scripts/guardrails.sh",
        "```",
        "",
        "Helpful references for contributors:",
        f"- [Contribution guide](https://github.com/{repo}/blob/main/CONTRIBUTING.md) (clean-room homebrew rules, provenance, AI disclosure)",
        f"- [Security policy](https://github.com/{repo}/blob/main/SECURITY.md)",
    ]
    if item_type == "pull request":
        lines[6:6] = [
            "Please ensure any code follows the clean-room homebrew policy (no console keys, firmware, or retail "
            "decryption code) and includes AI disclosure if generated with an AI assistant.",
            "",
        ]
    return "\n".join(lines).strip()


def main(env: Optional[Dict[str, str]] = None, runner: Optional[Runner] = None) -> int:
    env = dict(os.environ if env is None else env)
    runner = runner or run_gh
    repo = env.get("GH_REPO", "")
    author = env.get("AUTHOR_LOGIN", "")
    number = env.get("ISSUE_NUMBER", "")
    owner = env.get("REPO_OWNER", "hffgzktbcc-blip")
    is_pr = env.get("IS_PR") == "true"
    if not repo or not author or not number:
        print("Missing required environment variables; skipping.")
        return 0
    if should_skip_author(author, owner):
        print(f"Skipping welcome for author {author}.")
        return 0

    try:
        items: List[dict] = json.loads(runner(["api", f"repos/{repo}/issues?creator={author}&state=all&per_page=100"]))
        if any(str(i.get("number")) != str(number) for i in items):
            print(f"Author {author} has prior contributions; skipping welcome.")
            return 0
    except Exception as e:
        print(f"Warning: failed to query prior issues: {e}", file=sys.stderr)
        return 0

    try:
        comments: List[dict] = json.loads(runner(["api", f"repos/{repo}/issues/{number}/comments?per_page=100"]))
        if any(WELCOME_MARKER in c.get("body", "") for c in comments):
            print(f"Welcome comment already present on #{number}; skipping.")
            return 0
    except Exception as e:
        print(f"Warning: failed to fetch comments: {e}", file=sys.stderr)
        return 0

    body = build_welcome(repo, "pull request" if is_pr else "issue")
    try:
        runner(["api", "-X", "POST", f"repos/{repo}/issues/{number}/comments", "-f", f"body={body}"])
    except Exception as e:
        print(f"Failed to post welcome comment: {e}", file=sys.stderr)
        return 1
    print(f"Successfully posted welcome comment on #{number}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
