# Contributing to ounce-hb

Thank you for contributing to ounce-hb. These rules apply to all contributions from the
first commit and are not negotiable.

---

## 1. Scope & Core Boundary

ounce-hb is dedicated **exclusively** to running unencrypted Nintendo Switch homebrew:
- Unencrypted **NRO** files and **NSO** modules built from source using [devkitPro](https://devkitpro.org/)
  and [libnx](https://github.com/switchbrew/libnx).

We maintain zero tolerance for retail/commercial software:
- **No keys, key handling, key derivation, or key parsing.**
- **No console firmware, system archives, or encrypted binaries.**
- **No NSP, XCI, or NCA files, and no container decryption.**
- **No circumvention of any technical protection measures.**

Pull requests, issues, or discussions regarding retail titles, keys, firmware, or copyright
circumvention will be closed and locked immediately without discussion.

A workflow (`.github/workflows/policy.yml`) checks new and edited items from non-maintainers
against this rule. It never closes or locks an item by default. A clear request is flagged for a
maintainer: it gets the `area: legal` and `needs-owner-decision` labels and one comment quoting the
rule. An ambiguous mention gets `needs-owner-decision` only. A maintainer decides whether a flagged
item is closed. A maintainer can unlock and reopen any item closed in error; see
[docs/GITHUB_TOOLING.md](docs/GITHUB_TOOLING.md).

---

## 2. Clean-Room Rule

ounce-hb is developed strictly under clean-room engineering principles:

### Permitted Sources
- **The switchbrew wiki** (`switchbrew.org`): public documentation of NRO/NSO file formats and Horizon OS ABI.
- **devkitPro and libnx**: open-source source code and C/C++ header definitions (verify individual license terms).
- **Arm Architecture Reference Manual** (ARMv8-A): official instruction set specifications and CPU semantics.
- **Synthetic tests**: test fixtures and inputs generated in-house from documented specifications.

### Prohibited Sources
- **Do not read, consult, or copy code from** Yuzu, Ryujinx, Suyu, Skyline, Strato, or any other Switch emulator or their forks (regardless of whether they are licensed under GPL, MIT, or proprietary terms).
- **Tainted Contributors**: If you have previously read or worked on code in any Switch emulator, you must disclose this upfront in your PR or issue, and you must not write or review code for the corresponding subsystem in ounce-hb.
- **Attribution**: Whenever implementing non-obvious behavior, cite the public specification or libnx header in a code comment or PR description.

See [docs/CLEAN_ROOM.md](docs/CLEAN_ROOM.md) for full policy details.

---

## 3. How to Claim Work

To ensure smooth collaboration and prevent duplicate effort:

1. **Browse Issues**: Check existing GitHub Issues and Milestones to see what work is open.
2. **Claiming an Issue**: Before investing substantial time in an implementation, comment on the relevant issue to state that you are working on it.
3. **Small & Focused**: Keep pull requests focused on a single logical change or bug fix. Avoid monolithic PRs that combine unrelated refactorings with feature work.
4. **Topic Branches**: Develop your changes on a dedicated topic branch off `main`.

---

## 4. Provenance & Binary Files

We do not accept opaque binary blobs:
- Any binary file added to the repository (e.g. test fixtures) must be built from committed source with a pinned toolchain.
- Every binary must be registered in [docs/PROVENANCE.md](docs/PROVENANCE.md) with its SHA-256 hash, build command, and pinned toolchain version.
- `scripts/guardrails.sh` validates that no unregistered binaries or forbidden key/crypto patterns exist in the repository.

---

## 5. AI Assistance Disclosure

The use of AI-assisted coding tools (e.g. Claude, GitHub Copilot, Codex) is permitted under strict conditions:
- **Mandatory Disclosure**: Disclose in your pull request description which AI tool or model was used and what role it played (or state "None").
- **Clean-Room Responsibility**: You are personally responsible for everything you commit. Never ask an AI model to summarize, translate, or reproduce code from other Switch emulators.
- **Understanding**: You must thoroughly understand and be able to explain any code generated with AI assistance.

---

## 6. Pre-Submission Checks

Before opening a pull request, run all formatting, linting, test, and guardrail checks locally:

```sh
# 1. Format check
cargo fmt --all -- --check

# 2. Strict Clippy check
cargo clippy --workspace --all-targets --locked -- -D warnings

# 3. Test suite
cargo test --workspace --locked

# 4. Guardrails self-test and full scan
scripts/guardrails.sh --self-test
scripts/guardrails.sh
```

In your pull request, explicitly state which commands you ran and confirm that they passed without warnings or errors.
