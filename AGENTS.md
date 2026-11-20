# Working on ounce-hb

## 0. Family roadmap and current state (read first)

Before selecting or starting work, read the **canonical [Ounce Family Nov 2026–Nov 2027 roadmap](https://github.com/hffgzktbcc-blip/ounce-core/blob/main/docs/roadmap/OUNCE_FAMILY_1_YEAR_ROADMAP.md)**. It coordinates ounce-core (MIT), ounce-hb (MIT), ounce-runtime (GPL-3.0-or-later), and deckstream (MIT), including cross-repo release, CI, report and compatibility tasks. This repository's local docs/wiki/Roadmap.md is supplementary, not a competing family plan.

**Verify live GitHub state** (current main, open/merged PRs and issues, published releases, CI) before treating a roadmap item, dated plan, handoff or checklist as unfinished. Check the live [HB](https://github.com/hffgzktbcc-blip/ounce-hb/releases/latest), [Core](https://github.com/hffgzktbcc-blip/ounce-core/releases/latest) and [Runtime](https://github.com/hffgzktbcc-blip/ounce-runtime/releases/latest) release pages before making a published-version claim; never re-tag or reimplement shipped work. Record changed task status with evidence in the active source, not another dated handoff. Current user instructions determine authorized scope; historical notes do not create new assignments. Respect license isolation: share CLI/JSON evidence and use subprocesses, never port GPL runtime code into MIT projects.

Guidelines for autonomous agents and contributors working in this repository.

---

## 1. Non-Negotiable Boundaries
- **Homebrew Only**: Only unencrypted NRO and NSO files built from source with devkitPro/libnx are in scope.
- **Zero Retail Content**: Never introduce keys, firmware, retail game containers (NSP, XCI, NCA), or decryption code.
- **Clean-Room Enforcement**: Never read, copy, or reference source code from Yuzu, Ryujinx, Suyu, or any other Switch emulators. Cite public specifications (switchbrew wiki, libnx, Armv8-A manual).
- **Safe Rust**: The codebase enforces `#![forbid(unsafe_code)]`. Do not introduce unsafe blocks.
- **Provenance**: Any binary file (test fixture, asset) must be built from committed source with a pinned toolchain and registered in [docs/PROVENANCE.md](docs/PROVENANCE.md).

---

## 2. Agent Workflow
- **Public docs ship with the change:** when a PR alters runnable behaviour, flags, installation or diagnostic output, update the affected README/usage, limits and `Unreleased` changelog in that same PR. Follow the [family public docs contract](https://github.com/hffgzktbcc-blip/ounce-core/blob/main/docs/PUBLIC_DOCS_CONTRACT.md). Explain what users can do in active verbs; do not turn old test numbers into new results. If no public docs update is needed, state `Docs: none — <reason>` in the PR body so the CI guard records the decision.
- **Orientation**: Direction for the family is the [ounce-core one-year roadmap](https://github.com/hffgzktbcc-blip/ounce-core/blob/main/docs/roadmap/OUNCE_FAMILY_1_YEAR_ROADMAP.md); this repo's milestones are [docs/wiki/Roadmap.md](docs/wiki/Roadmap.md) and releases are in [CHANGELOG.md](CHANGELOG.md). Python script tests live in `scripts/test_*.py`.
- **Inspect First**: Read relevant code and documentation before modifying files.
- **Scope Discipline**: Stay within your assigned scope. Preserve unrelated files and do not change files outside the task. `src/`, `tests/` and `.github/workflows/` are in scope when the task needs them.
- **Honest Claims**: Never fabricate compatibility, pass rates, or benchmarks. State current limits plainly.
- **Public Hygiene**: The repository is public. Do not commit private local paths, internal hostnames, or unvetted credentials.

---

## 3. Required Verification
Always verify the full check suite before committing or reporting completion:
```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/guardrails.sh --self-test
scripts/guardrails.sh
```

---

## 4. Commit Trailers
When instructed to record agent co-authorship, end commit messages with:
```
Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
```
