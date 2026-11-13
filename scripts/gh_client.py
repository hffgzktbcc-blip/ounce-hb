"""Minimal runner around the `gh` CLI shared by the repository automation scripts.

A runner is any callable that takes the argument list after `gh` and returns stdout as text.
Tests inject a fake runner, so no test ever reaches the network.
"""
import subprocess
from typing import Callable, List

Runner = Callable[[List[str]], str]


class GhError(RuntimeError):
    """A `gh` invocation exited non-zero."""


def run_gh(args: List[str]) -> str:
    """Run `gh` with GH_TOKEN/GITHUB_TOKEN taken from the environment (set by the workflow)."""
    proc = subprocess.run(["gh", *args], capture_output=True, text=True)
    if proc.returncode != 0:
        raise GhError(f"gh {' '.join(args[:3])} failed: {proc.stderr.strip()}")
    return proc.stdout
