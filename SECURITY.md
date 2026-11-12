# Security policy

## Reporting a vulnerability
Please report suspected vulnerabilities privately through GitHub's "Report a vulnerability"
(Security tab > Advisories) rather than a public issue. Include the input file or a way to
generate it, the version or commit, and the observed behaviour. Expect an acknowledgement
within a few days.

## Scope
ounce-hb parses untrusted homebrew files: crashes, hangs, out-of-bounds behaviour and
sandbox escapes in the loader or runtime are in scope. The main crate is `#![forbid(unsafe_code)]`.

Not handled here: requests involving retail software, keys, firmware or protection
circumvention. Those are out of scope for this project and are closed.
