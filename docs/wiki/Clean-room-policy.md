# Clean-Room Policy

The ounce-hb project enforces a strict clean-room engineering policy to ensure complete legal
and architectural independence.

---

## 1. Permitted Reference Materials

Contributors may consult only the following public sources:
- **Switchbrew Wiki** (`switchbrew.org`): Public documentation of Switch file formats (`NRO0`, `NSO0`) and Horizon OS ABI.
- **devkitPro and libnx**: The open-source C/C++ toolchain, headers, and public examples.
- **Arm Architecture Documentation**: Official Armv8-A / Armv9-A Reference Manuals.
- **Independent In-House Tests**: Synthetic test files and unit tests created within this project.

When implementing non-obvious binary structures or behaviors, cite the public source in a code
comment or PR description.

---

## 2. Forbidden Materials

Contributors must **never** consult, read, or copy from:
- **Other Switch Emulators**: Code or documentation from Yuzu, Ryujinx, Suyu, Skyline, Strato, or any forks.
- **Nintendo SDKs**: Leaked or confidential developer documentation, headers, or sample code.
- **Retail Game Assets**: Game dumps, retail saves, keys, or proprietary firmware files.

---

## 3. Tainted Contributors

If you have previously inspected or contributed to the source code of any prohibited Switch
emulator, you are considered tainted for that functional area. You must disclose this upon joining
the project and must not author or review code for the corresponding subsystems in ounce-hb.

---

## 4. Automated Enforcement

Our repository includes a best-effort lint, `scripts/guardrails.sh`, which runs in CI and rejects (it can be evaded, so review is still required):
- Any binary file not registered in [PROVENANCE.md](../PROVENANCE.md).
- Any attempt to commit `.keys`, `.firmware`, `.xci`, `.nsp`, or `.nca` files.
- Code containing patterns associated with console keys or retail container decryption.

See the comprehensive [CLEAN_ROOM.md](../CLEAN_ROOM.md) document in the main repository for full details.
