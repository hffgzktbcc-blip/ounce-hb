# Project Roadmap

Every milestone is a concrete, checkable capability increment, and each open item is a GitHub issue. We do
not mark anything complete without a committed automated test or a recorded run. The current verified state is
in [LIMITS.md](../LIMITS.md).

---

## M0: Scaffold, runner and diagnostics (done)

- [x] Classify unencrypted `NRO0` and `NSO0` homebrew; refuse retail formats (`PFS0`, `HEAD`, `NCA0/2/3`, `META`).
- [x] Run an NRO through the released `ounce-core` CLI as a supervised subprocess (headless, wall-clock
  timeout, output captured).
- [x] Explain how a run stopped in plain words, with a reproduce command and a stable, documented exit code per
  category ([DIAGNOSIS.md](../DIAGNOSIS.md)).
- [x] Refuse guest arguments that ounce-core would mishandle (leading `-`, over 16 KiB; see
  [ounce-core #418](https://github.com/hffgzktbcc-blip/ounce-core/issues/418)).
- [x] End-to-end tests against the real ounce-core v0.8.0 release binary on Linux and macOS.
- [x] Clean-room and provenance guardrails, `#![forbid(unsafe_code)]`, CI on Linux, macOS and Windows, dependency
  and licence checks, draft-only releases.
- [x] Community files, issue forms and this wiki.

## M1: Corpus parity ([#7](https://github.com/hffgzktbcc-blip/ounce-hb/issues/7)) (in progress)

Run ounce-core's 50 registered corpus programs directly and through `ounce-hb run`, compare how each stops, and
keep a baseline that CI checks nightly. It says exactly what is and is not compared. It does not claim
first-frame-hash oracle parity, because the CLI does not expose frames.

## Parked: in-process library backend ([#8](https://github.com/hffgzktbcc-blip/ounce-hb/issues/8))

Calling ounce-core as a library needs a `[lib]` target and a headless run loop in ounce-core
([OUNCE_CORE_API_NEEDS.md](../OUNCE_CORE_API_NEEDS.md)). It is large and only worthwhile if the subprocess
path becomes the bottleneck, so it waits on an owner decision. The `Backend` trait is the seam.

## M2: Programs that need more than the console ([#9](https://github.com/hffgzktbcc-blip/ounce-hb/issues/9), [#10](https://github.com/hffgzktbcc-blip/ounce-hb/issues/10))

- [ ] Run libnx-built NSO programs ([#9](https://github.com/hffgzktbcc-blip/ounce-hb/issues/9)).
- [ ] For each libnx example, list from its source which services it needs, then close the gaps one program at a
  time with a recorded expected result ([#10](https://github.com/hffgzktbcc-blip/ounce-hb/issues/10)).
- [ ] Graphics and input beyond the headless CLI path.

## M3: Public preview ([#11](https://github.com/hffgzktbcc-blip/ounce-hb/issues/11))

First tagged release (a draft that the owner publishes), a short tutorial that builds a program with devkitPro
and runs it here, a fresh-clone walkthrough on macOS, Linux and Windows, and the M1 parity numbers.

---

## Help wanted

[Add a libnx example to the test corpus with provenance](https://github.com/hffgzktbcc-blip/ounce-hb/issues/12)
is a good first issue. See [ADD_A_PROGRAM.md](../ADD_A_PROGRAM.md).
