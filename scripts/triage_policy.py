#!/usr/bin/env python3
"""Enforce project rule 6 (CONTRIBUTING.md section 1): retail, key, firmware and circumvention
requests are closed and locked; ambiguous mentions are only labelled for the owner.

Called by .github/workflows/policy.yml for new issues, pull requests and discussions. The
untrusted text arrives only through environment variables, and this script never checks out or
runs code from the item it is judging.

Verdicts:
  close                 a strong match: a sentence names a protected term AND asks for it, with
                        no negation. The item gets a comment quoting the policy, the
                        'area: legal' label, is closed as not planned, and is locked.
  needs-owner-decision  any other protected-term mention (for example a bug report that merely
                        mentions firmware). The item gets the label only, and no comment.
  none                  no protected term.

Environment: GH_REPO, REPO_OWNER, EVENT_KIND (issue|pull_request|discussion), ITEM_NUMBER,
ITEM_NODE_ID (discussions only), ITEM_AUTHOR, ITEM_AUTHOR_TYPE, ITEM_TITLE, ITEM_BODY, GH_TOKEN.
"""
import json
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Tuple

sys.path.insert(0, str(Path(__file__).resolve().parent))

from gh_client import Runner, run_gh  # noqa: E402

POLICY_MARKER = "<!-- ounce-hb-policy-v1 -->"
LEGAL_LABEL = "area: legal"
REVIEW_LABEL = "needs-owner-decision"
POLICY_QUOTE = (
    "Pull requests, issues, or discussions regarding retail titles, keys, firmware, "
    "or copyright circumvention will be closed and locked immediately without discussion."
)

# A term names a protected subject. Each category is matched per sentence.
TERMS: Dict[str, "re.Pattern[str]"] = {
    "console keys": re.compile(
        r"\b(?:prod|title)[._\s-]?keys\b"
        r"|\bconsole\s+keys?\b"
        r"|\bkeys?\s+dumps?\b"
        r"|\bdumps?\s+(?:of\s+)?(?:the\s+|your\s+|my\s+)?(?:console\s+|switch\s+)?keys\b",
        re.IGNORECASE,
    ),
    "firmware": re.compile(r"\bfirmware\b|\bsystem\s+archives?\b", re.IGNORECASE),
    "retail files or games": re.compile(
        r"\b(?:nsp|xci|nca|ncz)\b"
        r"|\bretail\s+(?:games?|titles?|software|support|carts?)\b"
        r"|\bcommercial\s+(?:games?|titles?|software)\b",
        re.IGNORECASE,
    ),
    "piracy or circumvention": re.compile(
        r"\bpiracy\b|\bpirated?\b|\banti-?piracy\b|\bdrm\b|\bcircumvent\w*"
        r"|\bbypass\w*\s+(?:the\s+|any\s+|a\s+)?"
        r"(?:anti-?piracy|drm|copy[\s-]?protection|encryption|protection|authentication|eshop)\b",
        re.IGNORECASE,
    ),
}

# A request: the sentence asks for a protected thing, acquires it, or asks how to get or use it.
REQUEST = re.compile(
    r"\b(?:dump|dumps|dumping|download|downloads|downloading|share|sharing|upload|uploads|uploading"
    r"|obtain|crack|cracked|cracking|pirate|pirated|pirating|leak|leaked)\b"
    r"|\bsend\s+(?:me|us|your|the)\b"
    r"|\bgive\s+me\b"
    r"|\bprovide\s+(?:me\s+|us\s+)?(?:the\s+|your\s+|my\s+)?(?:keys?|firmware|nsp|xci|nca|games?|titles?|dumps?)\b"
    r"|\bwant\s+to\s+(?:run|play|load|use|get|dump|decrypt|pirate|crack|bypass|circumvent|download|install|boot)\b"
    r"|\b(?:where|how)\s+(?:do|can|to|should|would)\s+(?:i|we|one|you)\s+"
    r"(?:get|find|download|dump|obtain|decrypt|run|play|load|use|install|boot|crack|bypass|circumvent)\b"
    r"|\b(?:can|could|will|would)\s+you\s+(?:add|implement|enable)\b"
    r"|\badd(?:ing)?\s+(?:\w+\s+){0,3}support\b"
    r"|\bsupport\s+(?:for\s+)?(?:the\s+)?(?:retail|commercial|nsp|xci|nca|ncz|prod|title|firmware|keys?|games?|titles?)\b"
    r"|\b(?:i|we)\s+(?:need|want)\s+(?:the\s+|a\s+|some\s+|your\s+|my\s+)?"
    r"(?:keys?|firmware|nsp|xci|nca|retail|games?|titles?)\b",
    re.IGNORECASE,
)

# Negation or refusal in the same sentence turns a request into a discussion of the rule.
NEGATION = re.compile(
    r"\b(?:not|never|no|don'?t|doesn'?t|won'?t|cannot|can'?t|isn'?t|aren'?t|refuse|refuses|refused"
    r"|reject|rejects|rejected|block|blocks|blocked|prevent|prevents|prevented|shouldn'?t|should\s+not"
    r"|out\s+of\s+scope|unsupported|instead\s+of)\b",
    re.IGNORECASE,
)

HTML_COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)
# Checked and unchecked form and template checkboxes repeat the policy wording, so they are not evidence.
CHECKBOX_LINE = re.compile(r"^[ \t]*[-*][ \t]+\[[ xX]\].*$", re.MULTILINE)
SENTENCE_END = re.compile(r"[\n\r]+|[.!?]+(?:\s|$)")


@dataclass(frozen=True)
class Verdict:
    action: str  # "close", "needs-owner-decision" or "none"
    matches: Tuple[str, ...]


@dataclass(frozen=True)
class Target:
    kind: str  # "issue", "pull_request" or "discussion"
    repo: str
    number: str
    node_id: str
    author: str


def clean_text(title: str, body: str) -> str:
    """Return the text a human wrote: no HTML comments and no checkbox lines."""
    text = f"{title or ''}\n{body or ''}"
    text = HTML_COMMENT.sub("", text)
    return CHECKBOX_LINE.sub("", text)


def classify(title: str, body: str) -> Verdict:
    strong = set()
    weak = set()
    for sentence in SENTENCE_END.split(clean_text(title, body)):
        for name, term in TERMS.items():
            if not term.search(sentence):
                continue
            if REQUEST.search(sentence) and not NEGATION.search(sentence):
                strong.add(name)
            else:
                weak.add(name)
    if strong:
        return Verdict("close", tuple(sorted(strong)))
    if weak:
        return Verdict("needs-owner-decision", tuple(sorted(weak)))
    return Verdict("none", ())


def should_enforce(author: str, owner: str, author_type: str) -> bool:
    """Owner and bot accounts are never judged by the policy."""
    login = (author or "").strip().lower()
    if not login:
        return False
    if login == (owner or "").strip().lower():
        return False
    if (author_type or "").lower() == "bot" or login.endswith("[bot]"):
        return False
    return True


def policy_comment(repo: str) -> str:
    return "\n".join(
        [
            POLICY_MARKER,
            "This item is outside the scope of ounce-hb, so it has been closed and locked.",
            "",
            f"> {POLICY_QUOTE}",
            "",
            "ounce-hb runs only unencrypted Nintendo Switch homebrew (NRO and NSO) built from source with "
            f"devkitPro and libnx. See [CONTRIBUTING.md](https://github.com/{repo}/blob/main/CONTRIBUTING.md) "
            "section 1 for the scope rule.",
            "",
            "If this was closed in error, a maintainer can unlock and reopen it.",
        ]
    )


def _api(runner: Runner, args: Iterable[str]) -> str:
    return runner(["api", *args])


def _graphql(runner: Runner, query: str, **variables: str) -> str:
    fields: List[str] = []
    for key, value in variables.items():
        fields += ["-f", f"{key}={value}"]
    return runner(["api", "graphql", "-f", f"query={query}", *fields])


LABEL_ID_QUERY = """
query($owner: String!, $name: String!, $label: String!) {
  repository(owner: $owner, name: $name) { label(name: $label) { id } }
}
"""
ADD_LABEL = """
mutation($labelable: ID!, $label: ID!) {
  addLabelsToLabelable(input: {labelableId: $labelable, labelIds: [$label]}) { clientMutationId }
}
"""
ADD_COMMENT = """
mutation($discussion: ID!, $body: String!) {
  addDiscussionComment(input: {discussionId: $discussion, body: $body}) { comment { id } }
}
"""
CLOSE_DISCUSSION = """
mutation($discussion: ID!) {
  closeDiscussion(input: {discussionId: $discussion, reason: OUTDATED}) { discussion { id } }
}
"""
LOCK = """
mutation($lockable: ID!) {
  lockLockable(input: {lockableId: $lockable}) { lockedRecord { id } }
}
"""


def _discussion_label(runner: Runner, target: Target, label: str) -> None:
    owner, name = target.repo.split("/", 1)
    out = _graphql(runner, LABEL_ID_QUERY, owner=owner, name=name, label=label)
    label_id = (((json.loads(out).get("data") or {}).get("repository") or {}).get("label") or {}).get("id")
    if not label_id:
        raise RuntimeError(f"label {label!r} does not exist; run scripts/setup_github.sh --apply")
    _graphql(runner, ADD_LABEL, labelable=target.node_id, label=label_id)


def apply_verdict(runner: Runner, target: Target, verdict: Verdict) -> None:
    if verdict.action == "none":
        return
    if target.kind == "discussion":
        _apply_discussion(runner, target, verdict)
    else:
        _apply_item(runner, target, verdict)


def _apply_item(runner: Runner, target: Target, verdict: Verdict) -> None:
    base = f"repos/{target.repo}/issues/{target.number}"
    if verdict.action == "needs-owner-decision":
        _api(runner, ["-X", "POST", f"{base}/labels", "-f", f"labels[]={REVIEW_LABEL}"])
        return
    _api(runner, ["-X", "POST", f"{base}/comments", "-f", f"body={policy_comment(target.repo)}"])
    _api(runner, ["-X", "POST", f"{base}/labels", "-f", f"labels[]={LEGAL_LABEL}"])
    if target.kind == "pull_request":
        # Pull requests have no state_reason; they are closed without one.
        _api(runner, ["-X", "PATCH", f"repos/{target.repo}/pulls/{target.number}", "-f", "state=closed"])
    else:
        _api(
            runner,
            ["-X", "PATCH", base, "-f", "state=closed", "-f", "state_reason=not_planned"],
        )
    _api(runner, ["-X", "PUT", f"{base}/lock", "-f", "lock_reason=off-topic"])


def _apply_discussion(runner: Runner, target: Target, verdict: Verdict) -> None:
    if verdict.action == "needs-owner-decision":
        _discussion_label(runner, target, REVIEW_LABEL)
        return
    _graphql(runner, ADD_COMMENT, discussion=target.node_id, body=policy_comment(target.repo))
    _discussion_label(runner, target, LEGAL_LABEL)
    # GitHub has no "not planned" reason for discussions; OUTDATED is the closest.
    _graphql(runner, CLOSE_DISCUSSION, discussion=target.node_id)
    _graphql(runner, LOCK, lockable=target.node_id)


def target_from_env(env: Dict[str, str]) -> Optional[Target]:
    kind = env.get("EVENT_KIND", "")
    if kind not in ("issue", "pull_request", "discussion"):
        return None
    return Target(
        kind=kind,
        repo=env.get("GH_REPO", ""),
        number=env.get("ITEM_NUMBER", ""),
        node_id=env.get("ITEM_NODE_ID", ""),
        author=env.get("ITEM_AUTHOR", ""),
    )


def main(env: Optional[Dict[str, str]] = None, runner: Optional[Runner] = None) -> int:
    env = dict(os.environ if env is None else env)
    runner = runner or run_gh
    target = target_from_env(env)
    if target is None or not target.repo or not target.number:
        print("Missing item information; nothing to do.")
        return 0
    if not should_enforce(target.author, env.get("REPO_OWNER", ""), env.get("ITEM_AUTHOR_TYPE", "")):
        print(f"Author {target.author!r} is the owner or a bot; policy not applied.")
        return 0
    verdict = classify(env.get("ITEM_TITLE", ""), env.get("ITEM_BODY", ""))
    if verdict.action == "none":
        print(f"{target.kind} #{target.number}: no protected term; nothing to do.")
        return 0
    try:
        apply_verdict(runner, target, verdict)
    except Exception as e:  # report and fail the job so the run is visible
        print(f"Policy action failed on {target.kind} #{target.number}: {e}", file=sys.stderr)
        return 1
    print(f"{target.kind} #{target.number}: {verdict.action} ({', '.join(verdict.matches)}).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
