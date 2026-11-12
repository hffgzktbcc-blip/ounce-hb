# Contributing

Thanks for helping. These rules apply from the first commit and are not negotiable.

## Scope
Homebrew only (NRO, and NSO/NPDM produced by devkitPro/libnx). No NSP/XCI/NCA loading, no
decryption, no key handling, no firmware or system files, no circumvention of any technical
protection. Issues and pull requests about retail software, keys or piracy are closed and
locked without discussion.

## Clean-room rule
- Allowed sources: the switchbrew wiki, libnx/devkitPro source and headers (check each
  licence), Arm documentation, and our own tests.
- **Do not copy code from, and do not read the source of,** Yuzu, Ryujinx, Suyu or any other
  Switch emulator or their forks. If you have read it, say so and do not work on the
  affected area. A contributor who has read such code must not implement the matching part.
- Cite the public source for non-obvious behaviour in a comment or the PR description.

## Provenance rule
Binary files are accepted only if they are built from committed source with a pinned
toolchain and registered in [docs/PROVENANCE.md](docs/PROVENANCE.md). `scripts/guardrails.sh`
fails CI on unregistered binaries and on key/decryption code patterns. No keys, retail
content or firmware, ever, including in tests (use synthetic data).

## AI-assistance disclosure
AI tools are allowed. State in the PR description which tool helped and for what. You are
responsible for the result: you must understand it, it must follow the clean-room rule, and
you must not paste in code you cannot attribute. Do not ask an AI tool to reproduce emulator code.

## Checks before a PR
```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/guardrails.sh
```
Describe what you ran and what you did not.
