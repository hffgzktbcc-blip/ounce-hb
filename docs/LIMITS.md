# Limits & Current Capabilities

This document provides an honest, comprehensive account of what ounce-hb can do today, what it
cannot yet do, and what it will never do.

---

## 1. Permanent Non-Goals (Never Supported)

The following will never be supported by ounce-hb:
- **Retail Games & Commercial ROMs**: ounce-hb is strictly for unencrypted homebrew compiled from source.
- **Console Keys**: No key handling, derivation, or loading (`prod.keys`, `title.keys`, master keys, keyblobs).
- **Firmware & Encrypted Archives**: No support for proprietary Nintendo Switch system firmware or system archives.
- **Encrypted Containers**: Formats such as NSP / PFS0, XCI game card images, and NCA content archives are rejected immediately by format classification.
- **DRM Circumvention**: No bypasses of technical protection measures or encryption schemes.

---

## 2. Current M0 Capabilities

As of Milestone M0:
- **Format Classification**:
  - Classifies unencrypted homebrew `NRO0` executables (offset `0x10`).
  - Classifies unencrypted homebrew `NSO0` modules (offset `0x00`).
  - Detects and rejects retail containers with actionable error messages (exit code 2).
- **Subprocess Runner**:
  - Executes validated NRO files via an external `ounce-core` binary invoked in headless mode (`--present=headless`).
  - Resolves `ounce-core` via the `OUNCE_CORE_BIN` environment variable or the host system's `PATH`.
  - Captures and forwards guest stdout, stderr, and exit status.
  - Enforces a 60-second wall-clock timeout on guest runs. `OUNCE_HB_TIMEOUT_SECS` overrides it (greater than 0, at most 86400); see [DIAGNOSIS.md](DIAGNOSIS.md#timeout).
  - Refuses guest arguments starting with `-` to avoid silent argument dropping by upstream `ounce-core`.
  - Explains abnormal stops (unimplemented instruction, memory fault, svcBreak, unhandled SVC, budget, loader error, timeout, missing ounce-core) with a documented exit code per category; see [DIAGNOSIS.md](DIAGNOSIS.md).
- **Quality & Safety Invariants**:
  - Enforces `#![forbid(unsafe_code)]` in the main crate.
  - Automated guardrails script (`scripts/guardrails.sh`) ensures zero unregistered binaries and zero key/decryption code.

---

## 3. Current Developmental Limitations

The following features are not yet implemented in M0:
- **NSO Execution**: While NSO format classification is implemented, running NSO files directly through the runner is not yet supported.
- **Direct Library Integration**: ounce-hb currently runs `ounce-core` via a subprocess. Direct in-process linking is blocked until upstream `ounce-core` exposes a library target (see [OUNCE_CORE_API_NEEDS.md](OUNCE_CORE_API_NEEDS.md)).
- **Display & Graphics**: Execution is strictly headless (`--present=headless`). There is no virtual display, framebuffer presentation, or GPU pipeline.
- **Audio Output**: ounce-hb does no audio itself. ounce-core opens the default host audio device on every run (see Host Audio Noise below), and guest audio service emulation is ounce-core's, not ounce-hb's.
- **Interactive Input**: Controller, touch, or HID input handling is not implemented.
- **Networking**: BSD socket emulation and network communication services are not implemented.
- **32-bit Support**: Only 64-bit ARM (AArch64) homebrew executables are supported; 32-bit (AArch32) executables are not supported.
- **Guest-Argument Limit**: guest arguments may total at most 16 KiB (each argument's bytes plus one NUL). Longer sets are refused with exit code 2 before ounce-core starts. This works around [ounce-core #418](https://github.com/hffgzktbcc-blip/ounce-core/issues/418), where longer arguments overwrite the program's first instruction; see [DIAGNOSIS.md](DIAGNOSIS.md#guest-argument-limit).
- **Host Audio Noise**: ounce-core v0.8.0 opens the default audio device on every run and has no switch to skip it. On machines with no sound device it prints ALSA or `[HostAudioSink]` errors on stderr even for clean runs. Exit status and explanations are unaffected; see [DIAGNOSIS.md](DIAGNOSIS.md#host-audio-noise-on-stderr).
- **Stop Explanations**: Only the ounce-core v0.8.0 stop lines listed in [DIAGNOSIS.md](DIAGNOSIS.md) are recognised. Another ounce-core version may word them differently; such a run is reported as unrecognised with its output kept, not explained.
