#!/usr/bin/env python3
"""Apply project rule 6 (CONTRIBUTING.md section 1) to new and edited issues, pull requests and
discussions. Retail, key, firmware and circumvention requests are flagged for a maintainer.

Called by .github/workflows/policy.yml. The untrusted text arrives only through environment
variables, and this script never checks out or runs code from the item it is judging.

Verdicts:
  flag                  a strong match: a protected term with an acquisition or support verb
                        next to it, in one sentence with no negation. The item gets the
                        'area: legal' and 'needs-owner-decision' labels. When the item is newly
                        opened, it also gets the policy comment once.
  needs-owner-decision  any other protected-term mention (for example a bug report that merely
                        mentions firmware). The item gets the label only, and no comment.
  none                  no protected term.

The bot never closes or locks on its own. Only when the repository variable POLICY_AUTOCLOSE is
'true' does a newly opened flagged item also get closed and locked. An edited item only gets labels.

Environment: GH_REPO, REPO_OWNER, EVENT_KIND (issue|pull_request|discussion), EVENT_ACTION
(opened|edited|created), POLICY_AUTOCLOSE (optional), ITEM_NUMBER, ITEM_NODE_ID (discussions only),
ITEM_AUTHOR, ITEM_AUTHOR_TYPE, ITEM_TITLE, ITEM_BODY, GH_TOKEN.
"""
import json
import os
import re
import sys
import unicodedata
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

# Lookalike letters from other scripts that a reader cannot tell apart from Latin letters.
_CONFUSABLES = str.maketrans(
    {
        "а": "a", "е": "e", "о": "o", "р": "p", "с": "c", "у": "y", "х": "x", "і": "i",
        "ј": "j", "ѕ": "s", "һ": "h", "ԁ": "d", "А": "a", "В": "b", "Е": "e", "К": "k",
        "М": "m", "Н": "h", "О": "o", "Р": "p", "С": "c", "Т": "t", "Х": "x", "У": "y",
        "α": "a", "ε": "e", "ι": "i", "κ": "k", "ν": "v", "ο": "o", "ρ": "p", "τ": "t",
        "υ": "u", "Α": "a", "Β": "b", "Ε": "e", "Ι": "i", "Κ": "k", "Ν": "n", "Ο": "o",
        "Ρ": "p", "Τ": "t", "Υ": "y", "Χ": "x",
    }
)


def normalise(text: str) -> str:
    """Fold text so that look-alike spellings match the same patterns.

    NFKD folds fullwidth letters and ligatures. Combining marks and format characters (zero-width
    spaces, soft hyphens, bidi controls) are dropped. Cyrillic and Greek look-alikes become Latin.
    """
    folded = unicodedata.normalize("NFKD", text or "")
    kept = "".join(ch for ch in folded if unicodedata.category(ch) not in ("Mn", "Me", "Cf"))
    return kept.translate(_CONFUSABLES).lower()


# Checkbox labels and template lines copied verbatim from .github/ISSUE_TEMPLATE and
# .github/pull_request_template.md. They repeat the policy wording, so they are not evidence.
# Only these exact lines are removed. Every other line, checkbox and HTML comment is read.
# tests in test_triage_policy.py fail if a line here drifts from the templates.
BOILERPLATE_LINES = (
    "This issue does not involve retail software, keys, firmware, or circumventing protection.",
    "This program does not involve retail software, keys, firmware, or protection bypasses.",
    "It does not involve retail software, keys, firmware, or circumventing protection.",
    "**Homebrew only**: No keys, firmware, retail game assets, encrypted containers, or DRM circumvention",
)
_CHECKBOX_PREFIX = re.compile(r"^\s*[-*+]\s+\[[ xX]\]\s*")


def _line_key(line: str) -> str:
    text = _CHECKBOX_PREFIX.sub("", line).replace("*", "").replace("`", "")
    return " ".join(text.split()).lower()


_BOILERPLATE_KEYS = frozenset(_line_key(line) for line in BOILERPLATE_LINES)

# A term names a protected subject. The strong pattern for each category is in VERBS below.
TERMS: Dict[str, "re.Pattern[str]"] = {
    "console keys": re.compile(
        r"\b(?:prod|title)[^\w\n]{0,2}keys?\b"
        r"|\b(?:console|switch)\s+keys?\b"
        r"|(?P<term>keys?)\s+dumps?\b"
        r"|\bdumps?\s+(?:of\s+)?(?:the\s+|your\s+|my\s+)?(?:console\s+|switch\s+)?(?P<term2>keys?)\b"
    ),
    "firmware": re.compile(r"\bfirmwares?\b|\bofw\b|\bsystem\s+archives?\b"),
    "retail files or games": re.compile(
        r"\b(?:nsps?|xcis?|ncas?|nczs?)\b"
        r"|\b(?:n[\s._-]?s[\s._-]?p|x[\s._-]?c[\s._-]?i|n[\s._-]?c[\s._-]?[az])s?\b"
        r"|\bretail\s+(?:games?|titles?|software|carts?|cartridges?|discs?)\b"
        r"|\bretail\b(?=\s+support\b)"
        r"|\bcommercial\s+(?:games?|titles?|software)\b"
        r"|\b(?:game|games|cartridge|cartridges|cart|carts)\s+(?:dumps?|backups?|rips?)\b"
        r"|\bbackups?\s+of\s+(?:my\s+|the\s+|your\s+|our\s+)?(?:games?|carts?|cartridges?)\b"
    ),
    "piracy or circumvention": re.compile(
        r"\bpiracy\b|\bpirated?\b|\bpirating\b|\bcracked\b|\bcircumvent\w*|\bcopy[\s-]?protections?\b"
        r"|\bdrm\b(?!\s*/\s*kms\b)(?!\s+(?:backend|present|driver|subsystem|framebuffer|modeset|api|node|device|plane)\b)"
        r"|\bbypass\w*\s+(?:the\s+|any\s+|a\s+)?"
        r"(?P<term>anti-?piracy|drm|copy[\s-]?protection|encryption|protection|authentication|eshop)\b"
    ),
}

# Verbs that make a term a request. A verb must sit next to the term: at most two words before
# it, or directly after it. A word such as "firmware" in a bug report is then not a request.
_ACQUIRE = (
    r"download|downloads|downloading|dump|dumps|dumping|share|shares|sharing|send|sends|sending"
    r"|upload|uploads|uploading|obtain|obtaining|get|getting|find|provide|provides|providing|give"
    r"|leak|leaked|crack|cracked|cracking|pirate|pirated|pirating|decrypt|decrypting|grab|rip|ripping"
    r"|extract|extracting|need|needs|want|wants|support|supports|supporting"
)
_USE = r"run|runs|running|play|plays|playing|load|loads|loading|boot|boots|booting|install|installing|use|using"
_CIRCUMVENT = r"bypass\w*|circumvent\w*|remove|removing|strip\w*|defeat\w*|break|breaking|disable\w*"
VERBS: Dict[str, "re.Pattern[str]"] = {
    "console keys": re.compile(rf"\b(?:{_ACQUIRE})\b", re.IGNORECASE),
    "firmware": re.compile(rf"\b(?:{_ACQUIRE})\b", re.IGNORECASE),
    "retail files or games": re.compile(rf"\b(?:{_ACQUIRE}|{_USE})\b", re.IGNORECASE),
    "piracy or circumvention": re.compile(rf"\b(?:{_ACQUIRE}|{_CIRCUMVENT})\b", re.IGNORECASE),
}

# Negation or refusal in the same sentence turns a request into a discussion of the rule.
NEGATION = re.compile(
    r"\b(?:not|never|no|don'?t|doesn'?t|won'?t|cannot|can'?t|isn'?t|aren'?t|refuse|refuses|refused"
    r"|reject|rejects|rejected|block|blocks|blocked|prevent|prevents|prevented|shouldn'?t|should\s+not"
    r"|out\s+of\s+scope|unsupported|instead\s+of)\b",
    re.IGNORECASE,
)

SENTENCE_END = re.compile(r"[\n\r  ]+|[.!?]+(?:\s|$)")
MAX_VERB_GAP_WORDS = 2


@dataclass(frozen=True)
class Verdict:
    action: str  # "flag", "needs-owner-decision" or "none"
    matches: Tuple[str, ...]


@dataclass(frozen=True)
class Target:
    kind: str  # "issue", "pull_request" or "discussion"
    repo: str
    number: str
    node_id: str
    author: str


def clean_text(title: str, body: str) -> str:
    """Return the normalised text, without the policy boilerplate lines of the forms."""
    lines = normalise(f"{title or ''}\n{body or ''}").split("\n")
    return "\n".join(line for line in lines if _line_key(line) not in _BOILERPLATE_KEYS)


def _term_spans(pattern: "re.Pattern[str]", sentence: str) -> List[Tuple[int, int]]:
    """Spans of the protected word. A pattern may name the noun as a group, so a verb inside the
    phrase (the 'dump' in 'dump my keys') is not counted as part of the term."""
    spans = []
    for m in pattern.finditer(sentence):
        named = [g for g in ("term", "term2") if g in pattern.groupindex and m.group(g) is not None]
        spans.append(m.span(named[0]) if named else m.span())
    return spans


def _verb_binds(sentence: str, term_span: Tuple[int, int], verb_span: Tuple[int, int]) -> bool:
    t_start, t_end = term_span
    v_start, v_end = verb_span
    if v_end <= t_start:  # verb first: at most MAX_VERB_GAP_WORDS words between them
        return len(sentence[v_end:t_start].split()) <= MAX_VERB_GAP_WORDS
    if t_end <= v_start:  # term first: the verb must follow directly, as in "NSP support"
        return sentence[t_end:v_start].strip() == ""
    return False  # the two matches overlap, so one word is not a request for itself


def classify(title: str, body: str) -> Verdict:
    strong = set()
    weak = set()
    for sentence in SENTENCE_END.split(clean_text(title, body)):
        if not sentence.strip():
            continue
        negated = bool(NEGATION.search(sentence))
        for name, term in TERMS.items():
            term_spans = _term_spans(term, sentence)
            if not term_spans:
                continue
            verb_spans = [m.span() for m in VERBS[name].finditer(sentence)]
            bound = any(_verb_binds(sentence, t, v) for t in term_spans for v in verb_spans)
            if bound and not negated:
                strong.add(name)
            else:
                weak.add(name)
    if strong:
        return Verdict("flag", tuple(sorted(strong)))
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


def autoclose_enabled(env: Dict[str, str]) -> bool:
    """Closing and locking is opt-in: the repository variable POLICY_AUTOCLOSE must be 'true'."""
    return (env.get("POLICY_AUTOCLOSE") or "").strip().lower() == "true"


def policy_comment(repo: str, closed: bool) -> str:
    if closed:
        status = "This item is outside the scope of ounce-hb, so it has been closed and locked."
        recovery = "If this was closed in error, a maintainer can unlock and reopen it."
    else:
        status = (
            "This item touches the scope rule, so it has been labelled `area: legal` and "
            "`needs-owner-decision` for a maintainer to review. It has not been closed or locked."
        )
        recovery = "A maintainer decides whether it stays open."
    return "\n".join(
        [
            POLICY_MARKER,
            status,
            "",
            f"> {POLICY_QUOTE}",
            "",
            "ounce-hb runs only unencrypted Nintendo Switch homebrew (NRO and NSO) built from source with "
            f"devkitPro and libnx. See [CONTRIBUTING.md](https://github.com/{repo}/blob/main/CONTRIBUTING.md) "
            "section 1 for the scope rule.",
            "",
            recovery,
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


def _labels_for(verdict: Verdict) -> List[str]:
    if verdict.action == "flag":
        return [LEGAL_LABEL, REVIEW_LABEL]
    return [REVIEW_LABEL]


def _discussion_label(runner: Runner, target: Target, label: str) -> None:
    owner, name = target.repo.split("/", 1)
    out = _graphql(runner, LABEL_ID_QUERY, owner=owner, name=name, label=label)
    label_id = (((json.loads(out).get("data") or {}).get("repository") or {}).get("label") or {}).get("id")
    if not label_id:
        raise RuntimeError(f"label {label!r} does not exist; run scripts/setup_github.sh --apply")
    _graphql(runner, ADD_LABEL, labelable=target.node_id, label=label_id)


def apply_verdict(
    runner: Runner, target: Target, verdict: Verdict, *, autoclose: bool = False, new_item: bool = True
) -> None:
    """Act on a verdict. Only a newly opened item gets a comment, and only autoclose closes or locks."""
    if verdict.action == "none":
        return
    if target.kind == "discussion":
        _apply_discussion(runner, target, verdict, autoclose, new_item)
    else:
        _apply_item(runner, target, verdict, autoclose, new_item)


def _apply_item(runner: Runner, target: Target, verdict: Verdict, autoclose: bool, new_item: bool) -> None:
    base = f"repos/{target.repo}/issues/{target.number}"
    labels = _labels_for(verdict)
    _api(runner, ["-X", "POST", f"{base}/labels", *[arg for label in labels for arg in ("-f", f"labels[]={label}")]])
    if not new_item or verdict.action != "flag":
        return
    _api(runner, ["-X", "POST", f"{base}/comments", "-f", f"body={policy_comment(target.repo, autoclose)}"])
    if not autoclose:
        return
    if target.kind == "pull_request":
        # Pull requests have no state_reason; they are closed without one.
        _api(runner, ["-X", "PATCH", f"repos/{target.repo}/pulls/{target.number}", "-f", "state=closed"])
    else:
        _api(runner, ["-X", "PATCH", base, "-f", "state=closed", "-f", "state_reason=not_planned"])
    _api(runner, ["-X", "PUT", f"{base}/lock", "-f", "lock_reason=off-topic"])


def _apply_discussion(runner: Runner, target: Target, verdict: Verdict, autoclose: bool, new_item: bool) -> None:
    for label in _labels_for(verdict):
        _discussion_label(runner, target, label)
    if not new_item or verdict.action != "flag":
        return
    _graphql(runner, ADD_COMMENT, discussion=target.node_id, body=policy_comment(target.repo, autoclose))
    if not autoclose:
        return
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
    new_item = env.get("EVENT_ACTION", "opened") != "edited"
    try:
        apply_verdict(runner, target, verdict, autoclose=autoclose_enabled(env), new_item=new_item)
    except Exception as e:  # report and fail the job so the run is visible
        print(f"Policy action failed on {target.kind} #{target.number}: {e}", file=sys.stderr)
        return 1
    print(f"{target.kind} #{target.number}: {verdict.action} ({', '.join(verdict.matches)}).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
