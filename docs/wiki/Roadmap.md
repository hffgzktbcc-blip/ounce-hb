# Project Roadmap

This roadmap outlines the planned milestones for ounce-hb. Every milestone represents a concrete,
verifiable capability increment.

---

## Milestone M0: Scaffold & Subprocess Runner (Current)

**Objective**: Establish a strict clean-room foundation, robust CLI classification, guardrails CI,
and supervised subprocess execution.

- [x] Input classification for unencrypted `NRO0` and `NSO0` homebrew binaries.
- [x] Explicit rejection of retail formats (`PFS0`, `HEAD`, `NCA0/2/3`, `META`).
- [x] Subprocess runner backend invoking `ounce-core run-nro <app.nro> --present=headless`.
- [x] 60-second execution timeout protection and pipe drainage.
- [x] Rejection of guest arguments beginning with `-`.
- [x] Legal and binary provenance guardrails (`scripts/guardrails.sh`).
- [x] `#![forbid(unsafe_code)]` compliance.
- [x] Community infrastructure: clean-room policy, issue templates, PR template, wiki.

---

## Milestone M1: Upstream Library Integration

**Objective**: Link directly to `ounce-core` as an in-process Rust library rather than spawning child processes.

- [ ] Support upstream `ounce-core` `[lib]` target when released (see [OUNCE_CORE_API_NEEDS.md](../OUNCE_CORE_API_NEEDS.md)).
- [ ] In-process NRO loading, relocation mapping, and execution loop.
- [ ] Direct capture of guest console output and error diagnostics.
- [ ] Elimination of subprocess spawn overhead and temporary staging files.

---

## Milestone M2: Headless Homebrew Runtime & Services

**Objective**: Expand libnx standard library support and headless system call coverage.

- [ ] Support for running uncompressed NSO modules.
- [ ] Enhanced standard I/O redirection (printf console output).
- [ ] Filesystem redirection mapping (RomFS / SDMC folder mounting).
- [ ] Comprehensive stubs for common homebrew services (`set`, `time`, `hid` queries).

---

## Milestone M3: Interactive & Graphical Execution

**Objective**: Support interactive graphical homebrew software.

- [ ] Headless framebuffer capture and pixel comparison for automated tests.
- [ ] Optional minimal display presentation window for interactive execution.
- [ ] Host keyboard and gamepad mapping for libnx HID controller inputs.

---

## Guiding Rule: Evidence-Based Milestones

We do not mark milestones or features complete without committed automated tests verifying the
behavior. Track our current verifiable state in [LIMITS.md](../LIMITS.md).
