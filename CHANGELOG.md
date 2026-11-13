# Changelog

All notable changes to ounce-hb will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [0.0.1] - 2026-11-13

First tagged release: milestone M0. It runs unencrypted Switch homebrew through the released `ounce-core`
command-line tool and explains how each run stopped. It is a thin, tested layer, not an emulator of its own.
Tested against ounce-core v0.8.0.

### Added
- `ounce-hb run <app.nro> [-- guest args...]`: classify the file, then run it through the `ounce-core` binary
  (found through `OUNCE_CORE_BIN`, then `PATH`) in headless mode, with a wall-clock timeout (60 s by default,
  `OUNCE_HB_TIMEOUT_SECS` to change it) and the program's output forwarded.
- Plain-words explanation of how a run stopped, with the command to reproduce it and a stable, documented exit
  code per category ([docs/DIAGNOSIS.md](docs/DIAGNOSIS.md)). Backed by real runs against ounce-core v0.8.0 on
  Linux, macOS and Windows (18 end-to-end tests per OS, none skipped), except exit code 15 (unreachable from a
  homebrew program; the proof is in the document) and exit code 1, which is covered only by killing a run.
- Format classification of `NRO0` and `NSO0`, and refusal of retail and system formats (`PFS0`, `HEAD`, `NCA0/2/3`,
  `META`). NSO files are recognised but not executed.
- Guest-argument checks: arguments starting with `-` and argument sets over 16 KiB are refused before ounce-core
  starts. The 16 KiB limit works around ounce-core issue #418, which ounce-core v0.8.1 fixes (this release has
  not yet been tested against v0.8.1).
- Clean-room and provenance guardrails (`scripts/guardrails.sh`), a binary provenance register,
  `#![forbid(unsafe_code)]`.
- CI on Linux, macOS and Windows, licence and vulnerability checks, Dependabot, draft-only releases with provenance
  attestation, wiki publishing from `docs/wiki/`, and automation that closes and locks out-of-scope requests
  (retail games, keys, firmware, circumvention). That automation is covered by offline tests; its live behaviour
  on a real outside contribution has not been observed.
- Community files, issue forms, a roadmap and a wiki.

### Known limits
- No in-process `ounce-core` library backend (parked, see [issue #8](https://github.com/hffgzktbcc-blip/ounce-hb/issues/8)).
- No recorded corpus parity yet ([issue #7](https://github.com/hffgzktbcc-blip/ounce-hb/issues/7)).
- No NSO execution, graphics or input beyond what the ounce-core CLI provides in headless mode.
