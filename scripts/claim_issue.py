#!/usr/bin/env python3
"""Handle `/claim`-style comments on issues labelled 'good first issue' (self-assignment).

Called by .github/workflows/claim.yml. The comment text is untrusted: it is read from
COMMENT_BODY and matched against fixed phrases, and it is never executed or echoed back.

Environment: GH_REPO, ISSUE_NUMBER, ISSUE_STATE, ISSUE_ASSIGNEES (JSON), ISSUE_LABELS (JSON),
COMMENT_BODY, COMMENT_AUTHOR, GH_TOKEN.
"""
import json
import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Optional, Set

sys.path.insert(0, str(Path(__file__).resolve().parent))

from gh_client import Runner, run_gh  # noqa: E402

CLAIM_MARKER = "<!-- ounce-hb-claim-v1 -->"
CLAIM_PHRASES = {
    "i'll take this",
    "ill take this",
    "i will take this",
    "can i work on this",
    "can i take this",
    "claim",
    "/claim",
    "i would like to work on this",
    "i'd like to work on this",
    "id like to work on this",
    "i want to work on this",
}


def normalize(text: str) -> str:
    cleaned = text.strip().lower()
    if cleaned.startswith("/"):
        return "/" + re.sub(r"^[^\w/]+|[^\w]+$", "", cleaned[1:])
    return re.sub(r"^[^\w]+|[^\w]+$", "", cleaned)


def is_claim(comment_body: str) -> bool:
    if normalize(comment_body) in CLAIM_PHRASES:
        return True
    return any(normalize(line) in CLAIM_PHRASES for line in comment_body.splitlines())


def parse_logins(raw: str) -> List[str]:
    try:
        items = json.loads(raw or "[]")
    except ValueError:
        return []
    return [i.get("login") if isinstance(i, dict) else str(i) for i in items]


def parse_labels(raw: str) -> Set[str]:
    try:
        items = json.loads(raw or "[]")
    except ValueError:
        return set()
    return {(i.get("name", "") if isinstance(i, dict) else str(i)).lower() for i in items}


def browse_url(repo: str) -> str:
    return (
        f"https://github.com/{repo}/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22+no%3Aassignee"
    )


def main(env: Optional[Dict[str, str]] = None, runner: Optional[Runner] = None) -> int:
    env = dict(os.environ if env is None else env)
    runner = runner or run_gh
    repo = env.get("GH_REPO", "")
    number = env.get("ISSUE_NUMBER", "")
    comment_body = env.get("COMMENT_BODY", "")
    author = env.get("COMMENT_AUTHOR", "")
    state = env.get("ISSUE_STATE", "open").lower()
    if not repo or not number or not author or not comment_body:
        print("Missing required environment variables; nothing to do.")
        return 0
    if not is_claim(comment_body):
        print("Comment is not a claim request; skipping.")
        return 0
    if state != "open":
        print(f"Issue #{number} is not open ({state}); skipping.")
        return 0

    def post(text: str) -> None:
        runner(["api", "-X", "POST", f"repos/{repo}/issues/{number}/comments", "-f", f"body={text}"])

    assignees = parse_logins(env.get("ISSUE_ASSIGNEES", "[]"))
    if assignees:
        if author in assignees:
            print(f"Issue #{number} is already assigned to {author}.")
            return 0
        post(
            f"{CLAIM_MARKER}\n"
            f"This issue is already assigned to @{assignees[0]}.\n\n"
            f"You can browse other starter issues with the 'good first issue' label here:\n{browse_url(repo)}"
        )
        return 0

    if "good first issue" not in parse_labels(env.get("ISSUE_LABELS", "[]")):
        post(
            f"{CLAIM_MARKER}\n"
            "Self-assignment is available for issues labeled 'good first issue'. "
            "For other issues, a maintainer will coordinate assignment.\n\n"
            f"You can browse open beginner tasks here:\n{browse_url(repo)}"
        )
        return 0

    try:
        runner(["api", "-X", "POST", f"repos/{repo}/issues/{number}/assignees", "-f", f"assignees[]={author}"])
        post(
            f"{CLAIM_MARKER}\n"
            f"Assigned to @{author}. Tell us if you get stuck! "
            "If it goes quiet for 14 days we will check whether to keep it reserved."
        )
    except Exception as e:
        print(f"Failed to assign issue: {e}", file=sys.stderr)
        return 1
    print(f"Assigned #{number} to @{author}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
