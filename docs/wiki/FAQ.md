# Frequently Asked Questions (FAQ)

### What is ounce-hb?
`ounce-hb` is a focused, clean-room runner for unencrypted Nintendo Switch homebrew binaries
(specifically `.nro` files and `.nso` modules) built from source with devkitPro/libnx. It provides
a lightweight CLI to test and verify homebrew without requiring a physical console or complex
emulation setups.

---

### Can ounce-hb run commercial retail games?
**No, absolutely not.** ounce-hb is architected strictly for homebrew applications. Commercial
retail formats (such as NSP/PFS0 packages, XCI game card images, and NCA content archives) are
explicitly rejected by the format classifier with an exit code of 2.

---

### Does ounce-hb require console keys or firmware?
**No.** ounce-hb requires zero console keys (`prod.keys`, `title.keys`), zero master keys, and zero
console firmware files. It never decrypts encrypted data and contains no key handling or crypto
derivation routines. In fact, our automated guardrails (`scripts/guardrails.sh`) actively reject any
commit that introduces key-handling or decryption patterns.

---

### How does ounce-hb relate to ounce-core?
`ounce-core` is the upstream verification and emulation engine (providing ARM64 CPU interpreters,
recompilers, MMU, and Horizon kernel stubs). `ounce-hb` provides the homebrew-specific application
loader, CLI frontend, guest argument handling, and runner orchestration.

---

### Why does ounce-hb use a subprocess runner instead of linking to ounce-core?
In milestone M0, ounce-core has not yet exposed a public Rust library (`[lib]`) target; its
functionality lives in its binary crate. ounce-hb solves this by executing `ounce-core run-nro ...`
as a supervised child process with timeout protection and output capture. Once ounce-core introduces
a library target, ounce-hb will add an in-process backend (see [OUNCE_CORE_API_NEEDS.md](../OUNCE_CORE_API_NEEDS.md)).

---

### What are the rules for contributing?
1. **Clean Room**: Never read or adapt code from other Switch emulators (e.g. Yuzu, Ryujinx, Suyu).
2. **Permitted Sources**: Base all work on switchbrew wiki, libnx headers, and Arm manuals.
3. **Homebrew Only**: Never submit retail game content or keys.
4. **Guardrails**: All tests, clippy checks, and `scripts/guardrails.sh` must pass.

See [Clean-Room Policy](Clean-room-policy.md) and [CONTRIBUTING.md](../../CONTRIBUTING.md) for
complete details.
