# Working on ounce-hb

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
- **Inspect First**: Read relevant code and documentation before modifying files.
- **Scope Discipline**: Stay within your assigned scope. Preserve unrelated files and do not touch `src/`, `tests/`, or `.github/workflows/` unless explicitly instructed.
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
