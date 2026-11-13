# Security Policy

## Reporting a Vulnerability

Please report suspected security vulnerabilities privately through GitHub's
[Private Vulnerability Reporting](https://github.com/hffgzktbcc-blip/ounce-hb/security/advisories/new)
(available under the repository's **Security** tab > **Report a vulnerability**), rather than in a
public issue or discussion.

When submitting a report, please include:
1. A description of the vulnerability and its potential impact.
2. The specific commit or version of ounce-hb tested.
3. Steps to reproduce, including a minimal reproducer file or instructions to generate one from source.
4. Host OS and architecture details.

You can expect an initial acknowledgement within 3–5 business days.

---

## Scope

ounce-hb parses untrusted homebrew binaries and executes guest code via the runner backend. The
following are in scope:
- **Loader vulnerabilities**: memory safety bugs, panics, out-of-bounds reads, or unbounded resource consumption during NRO/NSO parsing and validation.
- **Runner vulnerabilities**: command or argument injection, improper pipe handling, or unintended host filesystem access in the runner backend.
- **Guest sandbox escapes**: unintended modification of host memory or filesystem outside designated guest paths.
- **Safety invariants**: the main crate enforces `#![forbid(unsafe_code)]`; any violation of safe Rust invariants is treated with high priority.

---

## Out of Scope

The following are strictly out of scope:
- Inquiries or reports involving retail Nintendo Switch games, console keys, firmware, or copyright protection circumvention. ounce-hb does not support, contain, or process retail assets.
- Ordinary logic bugs, missing HLE services, or homebrew compatibility defects that do not have security or isolation implications (please report these as standard issues).
- Vulnerabilities in third-party toolchains (e.g. devkitPro or GCC/Clang) unless directly exploitable through ounce-hb.
