# Project Vision

**Execute Nintendo Switch homebrew with verifiable correctness, complete transparency, and zero legal ambiguity.**

---

## Why ounce-hb?

Most tools in the console emulation space prioritize booting commercial retail games. That focus
introduces massive architectural complexity, proprietary encryption dependencies, key handling,
firmware requirements, and continuous legal scrutiny.

Homebrew software is fundamentally different. Open-source developers build homebrew using
[devkitPro](https://devkitpro.org/) and [libnx](https://github.com/switchbrew/libnx) from source
code they own. They need:
1. **A lightweight, scriptable runner** that executes on standard development workstations (macOS, Linux).
2. **Deterministic diagnostics** with clear console logging and meaningful exit codes, rather than silent hangs or crashes.
3. **Zero proprietary prerequisites**: no console keys, no dumped firmware, and no commercial game assets.
4. **An unassailable legal foundation** built strictly through clean-room development.

ounce-hb was created to meet these exact needs.

---

## Three Pillars

### 1. Clean-Room Integrity & Provenance
Every line of code in ounce-hb is developed from public documentation, official Arm architecture
manuals, and open-source libnx headers. We maintain an absolute separation from existing Switch
emulators. Every binary fixture is tracked with cryptographic hashes in our provenance register.

### 2. Developer Ergonomics & Diagnostic Clarity
Running a homebrew application should be as simple as `ounce-hb run app.nro`. When something fails,
ounce-hb explains why: distinguishing between malformed headers, unsupported system calls, and
runtime timeouts.

### 3. Solid Foundation on ounce-core
Rather than reinventing ARM64 execution engines and MMU translation, ounce-hb builds upon
[ounce-core](https://github.com/hffgzktbcc-blip/ounce-core). ounce-hb acts as the homebrew
application runtime, managing format loading and guest argument passing, while delegating low-level
CPU and kernel emulation to ounce-core.

---

## Phased Roadmap

| Milestone | Target | Description | Status |
|---|---|---|---|
| **M0** | Scaffold, runner, diagnostics | Format classification, retail-container refusal, guardrails CI, supervised `ounce-core` subprocess runner, plain-words stop diagnosis with stable exit codes, end-to-end tests against ounce-core v0.8.0. | **Done** |
| **M1** | Corpus parity | Run ounce-core's registered corpus directly and through `ounce-hb`, compare how each stops, keep a nightly-checked baseline ([#7](https://github.com/hffgzktbcc-blip/ounce-hb/issues/7)). | In progress |
| **M2** | More than the console | NSO programs, and services, graphics and input for the libnx examples, one program at a time ([#9](https://github.com/hffgzktbcc-blip/ounce-hb/issues/9), [#10](https://github.com/hffgzktbcc-blip/ounce-hb/issues/10)). | Planned |
| **M3** | Public preview | First release, tutorial, cross-platform walkthrough, parity numbers ([#11](https://github.com/hffgzktbcc-blip/ounce-hb/issues/11)). | Planned |

An in-process `ounce-core` library backend is parked until the owner decides it is worth the work
([#8](https://github.com/hffgzktbcc-blip/ounce-hb/issues/8)); see [OUNCE_CORE_API_NEEDS.md](OUNCE_CORE_API_NEEDS.md).

---

## Verifiable Claims Standard

In ounce-hb, we make no claims without reproducible evidence:
- We never claim compatibility with a program without an automated test or recorded execution log.
- We plainly state current limitations in [LIMITS.md](LIMITS.md).
- We maintain automated guardrails to ensure our clean-room boundaries are never compromised.
