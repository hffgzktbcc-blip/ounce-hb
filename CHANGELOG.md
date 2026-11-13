# Changelog

All notable changes to ounce-hb will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [0.0.1] - 2026-11-13

### Added
- **CLI and Runner**:
  - `ounce-hb run <app.nro> [-- guest args...]` command to classify and run homebrew NRO binaries.
  - Subprocess execution runner that invokes the upstream `ounce-core` binary in headless mode (`--present=headless`).
  - Discovery of `ounce-core` via the `OUNCE_CORE_BIN` environment variable or system `PATH`.
  - 60-second execution timeout to prevent runaway guest processes.
  - Validation refusing guest arguments beginning with `-` (to prevent silent argument loss in upstream core).
- **Format Classification**:
  - Magic byte classification for unencrypted homebrew `NRO0` and `NSO0` formats.
  - Explicit rejection with detailed error messages for out-of-scope retail and system formats:
    - `PFS0` / NSP package files.
    - `HEAD` / XCI game card images.
    - `NCA0`, `NCA2`, `NCA3` content archives.
    - `META` / NPDM permission metadata files.
- **Guardrails & Verification**:
  - Provenance and legal guardrails script (`scripts/guardrails.sh`) preventing unregistered binary files and blocking key/decryption code patterns.
  - Binary provenance register in [docs/PROVENANCE.md](docs/PROVENANCE.md).
  - `#![forbid(unsafe_code)]` enforced across the crate.
  - Automated unit and integration test suite (`src/lib.rs`, `tests/cli.rs`).
- **Community & Documentation Scaffolding**:
  - Project vision and milestone roadmap ([docs/VISION.md](docs/VISION.md)).
  - Comprehensive clean-room engineering policy ([docs/CLEAN_ROOM.md](docs/CLEAN_ROOM.md)).
  - Verified current capabilities and permanent non-goals ([docs/LIMITS.md](docs/LIMITS.md)).
  - Guide to building and running homebrew tests ([docs/ADD_A_PROGRAM.md](docs/ADD_A_PROGRAM.md)).
  - Detailed upstream library API requirements ([docs/OUNCE_CORE_API_NEEDS.md](docs/OUNCE_CORE_API_NEEDS.md)).
  - Full repository wiki pages ([docs/wiki/](docs/wiki/)).
  - Contributor guidelines ([CONTRIBUTING.md](CONTRIBUTING.md)), code of conduct ([CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)), and private security disclosure route ([SECURITY.md](SECURITY.md)).
  - GitHub issue templates (bug reports, program requests) and pull request template.
