# Clean-Room Engineering Policy

This document defines the clean-room engineering standards for ounce-hb. Compliance is
mandatory for all contributors and automated agents.

---

## 1. Motivation & Principles

ounce-hb is an independent runner for unencrypted Nintendo Switch homebrew built from source.
To guarantee complete legal and architectural independence, all code in this repository must
be developed cleanly from public specifications, open-source homebrew toolchains, and independent
implementation.

We do not emulate commercial games, we do not decrypt proprietary formats, and we do not reuse
code or architectural designs from other Switch emulators.

---

## 2. Permitted Sources

All code, data structures, and ABI models must be derived strictly from the following sources:

1. **Arm Architecture Reference Manual** (ARMv8-A / ARMv9-A):
   - Official CPU instruction encodings, system register specifications, and exception models.
2. **The Switchbrew Wiki** (`switchbrew.org`):
   - Public community documentation of homebrew executable file formats (`NRO0`, `NSO0`) and Horizon OS ABI conventions.
3. **devkitPro & libnx**:
   - The open-source `devkitA64` toolchain and `libnx` C/C++ runtime headers and source code (subject to checking each component's open-source license).
4. **Synthetic Test Data & In-House Tests**:
   - Deterministic test cases and synthetic binary headers generated directly from committed specification logic.

Whenever implementing non-obvious binary structures or system call handling, cite the specific
public document or libnx header in a comment or the pull request description.

---

## 3. Strictly Prohibited Sources

Under no circumstances may any contributor or tool consult or incorporate the following:

- **Other Switch Emulators**:
  - Do not read, quote, adapt, or copy code from Yuzu, Ryujinx, Suyu, Skyline, Strato, or any other Switch emulator or their downstream forks.
  - This prohibition applies regardless of the emulator's license (GPL, MIT, BSD, or proprietary).
- **Nintendo SDK & Proprietary Documentation**:
  - Leaked Nintendo SDK headers, source code, internal manuals, or developer documentation.
- **Retail Content & Keys**:
  - Console keys (`prod.keys`, `title.keys`), key derivation code, master keys, keyblobs, or AES crypto engines.
  - Retail game card dumps (`XCI`), installable packages (`NSP`/`PFS0`), or content archives (`NCA`).
  - Official console firmware files or system archives.

---

## 4. Tainted Contributor Policy

To maintain clean-room integrity:
- If a contributor has read, audited, or contributed to the source code of any prohibited Switch emulator, they are considered **tainted** with respect to the corresponding subsystem.
- Tainted contributors must explicitly disclose this background before contributing.
- Tainted contributors are strictly barred from authoring or reviewing code for the affected areas in ounce-hb.

---

## 5. Automated Verification & Guardrails

Clean-room and provenance rules are enforced through automated tooling:
- `scripts/guardrails.sh` is a best-effort lint that runs in CI and locally. It rejects the items below, but it only scans listed file types and fixed patterns and can be evaded, so it supports review and does not replace it:
  - Unregistered binary files.
  - Prohibited file extensions (`.nsp`, `.xci`, `.nca`, `.keys`, `.firmware`).
  - Code patterns associated with key derivation or retail container decryption.
- The binary provenance register is maintained in [docs/PROVENANCE.md](PROVENANCE.md).

---

## 6. Cross-Repository & Ounce Family Boundaries

`ounce-hb` is part of the four-repository Ounce family (`ounce-core`, `ounce-hb`, `ounce-runtime`, `deckstream`). Each repository enforces its own license and source boundary:

- **Subprocess isolation**: `ounce-hb` (MIT, `#![forbid(unsafe_code)]`) never links `ounce-core` (MIT) or `ounce-runtime` (GPL-3.0-or-later) in-process. Both engines are invoked strictly as external subprocesses (`--backend core|runtime|auto|path`).
- **One-way license boundary**: Do not copy GPL-3.0-or-later code from `ounce-runtime` into MIT `ounce-hb` or `ounce-core`. Cross-engine observations (such as `ounce-hb run --auto` repro manifests or `diff-trace` divergence reports) may be shared one-way as factual bug reports.
- **Distinct source policies**: `ounce-core` maintains its own sources and legal guardrails policy ([ounce-core `docs/CLEAN_ROOM.md`](https://github.com/hffgzktbcc-blip/ounce-core/blob/main/docs/CLEAN_ROOM.md), allowing attributed MIT ports). `ounce-hb` itself remains strictly clean-room under Sections 1–4 above (public specifications, switchbrew wiki, devkitPro/libnx, and synthetic fixtures only).
- **Shared non-negotiable legal boundary**: Across all Ounce repositories, console keys, firmware, retail containers (`NSP`/`XCI`/`NCA`), leaked SDK material, and DRM circumvention are permanently prohibited.
