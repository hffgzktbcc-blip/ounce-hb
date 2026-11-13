"""Test double for scripts/gh_client.py. Records every argument list and answers from a script.

Answers are looked up by the first argument string that appears in the call, in order. A value
that is an exception instance is raised, so tests can simulate a failed `gh` call.
"""
from typing import Any, List, Optional, Sequence, Tuple


class FakeGh:
    def __init__(self, answers: Optional[Sequence[Tuple[str, Any]]] = None):
        self.answers = list(answers or [])
        self.calls: List[List[str]] = []

    def __call__(self, args: List[str]) -> str:
        self.calls.append(list(args))
        joined = " ".join(args)
        for needle, answer in self.answers:
            if needle in joined:
                if isinstance(answer, Exception):
                    raise answer
                return answer
        return "[]" if "graphql" not in args else "{}"

    def calls_containing(self, needle: str) -> List[List[str]]:
        return [c for c in self.calls if needle in " ".join(c)]
