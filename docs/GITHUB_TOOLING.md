# GitHub Tooling and Repository Configuration

This document specifies the repository automation, automated workflow policies, and manual repository settings for `ounce-hb`. While GitHub Actions workflows and automation scripts are version-controlled in the repository, certain security, access, and feature settings must be configured manually by the repository owner or administrator in the GitHub web interface.

---

## Manual Repository Settings

The repository administrator must verify and configure the following settings under the repository's **Settings** tab.

### 1. Discussions
- **Navigation:** Settings → General → Features → Discussions
- **Action:** **Enable** (checked).
- **Recommended Categories:**
  - `General`: Open discussion regarding Nintendo Switch homebrew and `ounce-hb`.
  - `Q&A`: Developer questions on compiling libnx programs from source or running homebrew.
  - `Ideas`: Feature suggestions, architectural discussions, and compatibility requests.
  - `Show and tell`: Demonstrations of homebrew programs running on `ounce-hb`.
- **Policy:** Discussions should be used for open-ended questions, community interaction, and user troubleshooting to keep the GitHub Issues tracker focused strictly on reproducible bugs and development tasks.

### 2. Wiki
- **Navigation:** Settings → General → Features → Wikis
- **Action:** **Enabled.**
- **Source of truth:** the pages live in `docs/wiki/` and are reviewed in pull requests like any other file. The
  GitHub wiki is a published copy. GitHub only creates a wiki's git repository after a first page is saved in the
  browser, so that step is manual; after it, push the pages with relative links rewritten to wiki links:
  `[x](FAQ.md)` becomes `[x](FAQ)`, and `../../CONTRIBUTING.md` becomes a full
  `https://github.com/hffgzktbcc-blip/ounce-hb/blob/main/CONTRIBUTING.md` link.
- Do not edit the wiki directly; edit `docs/wiki/`. Merging a change under `docs/wiki/` republishes it through
  `.github/workflows/wiki-sync.yml` (`scripts/publish_wiki.py`; run it by hand with `--check` to preview).

### 3. Branch Protection (`main`)
- **Navigation:** Settings → Branches
- **Applied state (2026-11-13):**
  - A pull request is required to merge. Required approving reviews: 0 (a single maintainer, so a required review
    would block every change); conversations must be resolved.
  - Required status checks: `check (ubuntu-latest)`, `check (macos-latest)`, `check (windows-latest)`,
    `e2e (linux-x86_64)`, `e2e (macos-arm64)`. `Cargo deny` is not required because its workflow only runs when dependency files change, and a required check that never reports blocks every other PR. Branches need not be up to date.
  - Force pushes and branch deletion are blocked.
  - Not enforced for administrators, so the owner can still merge in an emergency.
- **Tighten when there is more than one maintainer:** require 1 approving review, require Code Owners review, and
  enable "Do not allow bypassing the above settings".

### 4. Pages
- **Navigation:** Settings → Pages
- **Action:** Set **Build and deployment Source** to **None** (disabled).
- **Note:** If a public documentation website or coverage report is published in the future, configure the Source as **GitHub Actions** rather than deploying from a static branch.

### 5. Code Security & Analysis
- **Navigation:** Settings → Code security and analysis (and the repository's **Security** tab)
- **Required Configurations:**
  - **Private vulnerability reporting:** **Enabled**. Allows external security researchers to privately disclose vulnerabilities directly to maintainers via the Security tab.
  - **Dependency graph:** **Enabled**. Required for Dependabot and Dependency Review workflows.
  - **Dependabot alerts:** **Enabled**.
  - **Dependabot security updates:** **Enabled**.
  - **Secret scanning:** **Enabled** with push protection enabled.

---

## Repository Labels

The repository organizes issues and pull requests using standard labels across areas, issue types, and triage priorities.

| Label Name | Hex Color | Purpose / Scope |
| :--- | :--- | :--- |
| `accessibility` | `#f143ab` | Barrier affecting people with disabilities |
| `bug` | `#d73a4a` | Confirmed problem or incorrect behavior |
| `documentation` | `#0075ca` | Improvements or additions to documentation |
| `duplicate` | `#cfd3d7` | Issue or pull request already tracked elsewhere |
| `enhancement` | `#a2eeef` | New feature or functional enhancement |
| `good first issue` | `#7057ff` | Beginner-friendly starter tasks eligible for contributor self-assignment |
| `help wanted` | `#008672` | Community assistance or extra attention requested |
| `invalid` | `#e4e669` | Out-of-scope report (e.g. retail games, keys, or piracy requests) |
| `question` | `#d876e3` | Request for information or clarification |
| `wontfix` | `#ffffff` | Issue or request that will not be addressed |
| `area: runtime` | `#ededed` | Runtime core and execution engine |
| `area: runner` | `#ededed` | CLI runner, guest program invocation, and environment setup |
| `area: loader` | `#c2e0c6` | Homebrew NRO and NSO parsing and ELF relocation |
| `area: verification` | `#ededed` | Test suites, test harnesses, and validation fixtures |
| `area: docs` | `#ededed` | Documentation, specifications, and project guides |
| `area: ci` | `#ededed` | GitHub Actions workflows, CI scripts, and automation |
| `area: legal` | `#b60205` | Clean-room policy, provenance records, and license compliance |
| `community` | `#0052cc` | Outside testers, external reproductions, and community engagement |
| `program` | `#0e8a16` | Homebrew program or standalone test workload |
| `priority: high` | `#d93f0b` | Critical defect or blocking milestone task |
| `priority: low` | `#c5def5` | Minor issue or low-priority enhancement |
| `needs-repro` | `#fef2c0` | Missing reproduction steps or sample homebrew binary |
| `needs-owner-decision` | `#f9d0c4` | Architectural or policy decision requiring repository owner resolution |
| `blocked` | `#000000` | Blocked on upstream changes (e.g. `ounce-core`) or external dependency |
| `dependencies` | `#0366d6` | Dependency update pull requests |
| `rust` | `#000000` | Rust codebase changes |
| `github_actions` | `#000000` | GitHub Actions workflow modifications |
| `skip-changelog` | `#ffffff` | Exclude pull request from generated release notes |

---

## Automated Label Synchronization

The repository includes a helper script, `scripts/setup_github.sh`, to idempotently create and synchronize all required labels using the GitHub CLI (`gh`).

### Usage

1. **Dry-Run (Default):**
   Preview labels without making any modifications on GitHub:
   ```sh
   ./scripts/setup_github.sh
   ```

2. **Apply Changes:**
   Create or update all labels on the repository:
   ```sh
   ./scripts/setup_github.sh --apply
   ```

3. **Specify Target Repository:**
   ```sh
   ./scripts/setup_github.sh --apply --repo=hffgzktbcc-blip/ounce-hb
   ```

---

## Automated Workflows Summary

The repository includes the following GitHub Actions workflows under `.github/workflows/`:

- **`ci.yml`**:
  - Matrix across Linux (`ubuntu-latest`), macOS (`macos-latest`), and Windows (`windows-latest`).
  - Executes format checking (`cargo fmt --check`), clippy linting (`cargo clippy --locked`), test suites (`cargo test --locked`), and legal/clean-room guardrails (`scripts/guardrails.sh`).
  - Configures Git to disable CRLF auto-conversion on Windows runners to ensure uniform LF line endings and consistent formatting checks.
- **`dependabot.yml`**:
  - Configured under `.github/dependabot.yml` for automated weekly Cargo and GitHub Actions version updates.
- **`dependency-review.yml`**:
  - Analyzes pull requests modifying dependencies to block known high-severity vulnerabilities before merging.
- **`rust-security.yml`**:
  - Runs `cargo audit` against the RustSec Advisory Database and `cargo deny check` using `deny.toml` on scheduled intervals and dependency-modifying pull requests.
- **`labeler.yml`** & **`.github/labeler.yml`**:
  - Automatically classifies and labels pull requests according to modified file paths.
- **`release.yml`**:
  - Triggers on version tags (`v*`). Builds multi-platform release binaries for Linux (`x86_64-unknown-linux-gnu`), macOS (`aarch64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`).
  - Packages release archives with SHA256 checksums and creates a **DRAFT release only** (preventing inadvertent public release publication).
- **`welcome.yml`**:
  - Logic: `scripts/welcome_contributor.py`. Posts a helpful onboarding comment on the first issue or pull request opened by a new contributor, pointing to clean-room policies and local test commands.
- **`claim.yml`**:
  - Logic: `scripts/claim_issue.py`. Handles `/claim` and self-assignment requests from contributors on issues labeled `good first issue`.
- **`issue-triage.yml`**:
  - Posts a missing-details comment on newly opened issues (program, version, command, output) and flags potential duplicates. It never closes or locks anything, and it skips items the scope policy will close.
  - Logic: `scripts/triage_issue.py`. A field counts as present only when the issue form section has content, or the text has a specific signal (a `.nro`/`.nso`/`.elf` name or GitHub repository URL; a `v1.2.3` tag, a `commit` hash or a hex hash with digits and letters; `cargo run`, `ounce-hb run`, or a shell fence; `exit code N`, `panicked`, `error:`, `stderr`). A bare word such as "app" or "version" does not count.
- **`policy.yml`**:
  - Applies the scope rule (CONTRIBUTING.md section 1) to newly opened issues and pull requests, and to newly created discussions, from anyone other than the repository owner or a bot. Logic and tests: `scripts/triage_policy.py`.
  - Uses `pull_request_target`. It checks out only the default branch, never the pull request, and passes all item text through environment variables.
  - Permissions: `contents: read`, `issues: write`, `pull-requests: write`, `discussions: write`.

### Scope policy behaviour

| Verdict | When | Action |
| :--- | :--- | :--- |
| `close` | A sentence names a protected term (console keys such as a key file or "keys dump", firmware, NSP/XCI/NCA or retail games, piracy, DRM or circumvention) **and** asks for it (for example "dump", "download", "share", "where can I get", "add support", "I want to run"), with no negation or refusal in that sentence. | Comment quoting the policy and linking CONTRIBUTING.md; label `area: legal`; close (issues as *not planned*; pull requests have no reason field; discussions use GitHub's *outdated* reason, since discussions have no *not planned*); lock. |
| `needs-owner-decision` | A protected term appears without a request, or with negation or refusal (for example "should refuse ... with a clear error"), or only in a bug report. | Label `needs-owner-decision` only. No comment, no close. |
| `none` | No protected term. | Nothing. |

- Checkbox lines (`- [x] ...`) and HTML comments are ignored, because issue forms and the pull request template repeat the policy wording there.
- The owner and bot accounts are never judged.
- Labels must exist in the repository; run `scripts/setup_github.sh --apply` first. Discussions need the label ID, so a missing label fails the run.
- The classifier is deliberately conservative. A false negative is labelled for the owner; only a clear request is closed.

### Reopening a wrongly closed item

Maintainers can reverse a mistaken closure. A locked item cannot be reopened until it is unlocked.

1. **Unlock:** on the item, select **Unlock conversation** (under the lock or conversation menu). For a pull request, use the same control in the pull request sidebar or conversation menu.
2. **Reopen:** select **Reopen issue**, **Reopen pull request**, or **Reopen discussion**.
3. **Clean up:** remove the `area: legal` label, and the `needs-owner-decision` label if it applies. Then reply to the author.

The bot's comment says a maintainer can unlock and reopen. The bot does not undo its own action.
