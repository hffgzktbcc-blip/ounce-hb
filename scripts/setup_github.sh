#!/usr/bin/env bash
# Sets up GitHub labels for ounce-hb using the gh CLI.
# Safe and idempotent: dry-run by default; pass --apply to create/update labels on GitHub.
set -euo pipefail

REPO="${GH_REPO:-hffgzktbcc-blip/ounce-hb}"
DRY_RUN=true

for arg in "$@"; do
  case "$arg" in
    --apply|--run|--execute)
      DRY_RUN=false
      ;;
    --dry-run)
      DRY_RUN=true
      ;;
    --repo=*)
      REPO="${arg#*=}"
      ;;
    -h|--help)
      echo "Usage: $0 [--dry-run | --apply] [--repo=OWNER/REPO]"
      echo "Default is --dry-run. Pass --apply to create or update labels on GitHub."
      exit 0
      ;;
    *)
      echo "Unknown option: $arg" >&2
      exit 1
      ;;
  esac
done

if ! command -v gh >/dev/null 2>&1; then
  echo "Error: gh CLI is required but not installed." >&2
  exit 1
fi

# Define labels: name|color|description
LABELS=(
  "accessibility|f143ab|Barrier affecting people with disabilities"
  "bug|d73a4a|Something isn't working"
  "documentation|0075ca|Improvements or additions to documentation"
  "duplicate|cfd3d7|This issue or pull request already exists"
  "enhancement|a2eeef|New feature or request"
  "good first issue|7057ff|Good for newcomers"
  "help wanted|008672|Extra attention is needed"
  "invalid|e4e669|This doesn't seem right"
  "question|d876e3|Further information is requested"
  "wontfix|ffffff|This will not be worked on"
  "area: runtime|ededed|Runtime and execution engine code"
  "area: runner|ededed|Homebrew runner, process invocation, and guest execution"
  "area: loader|c2e0c6|NRO and NSO executable loading and ELF handling"
  "area: verification|ededed|Test harnesses, test fixtures, and validation"
  "area: docs|ededed|Documentation and specifications"
  "area: ci|ededed|Workflows, checks, and release automation"
  "area: legal|b60205|Clean-room homebrew rules, provenance, and license compliance"
  "community|0052cc|Outside testers, reproductions, and community discussion"
  "program|0e8a16|Homebrew program or test workload"
  "priority: high|d93f0b|High priority task or blocking issue"
  "priority: low|c5def5|Nice to have; low priority"
  "needs-repro|fef2c0|Needs a reproducible homebrew sample or steps"
  "needs-owner-decision|f9d0c4|Waiting on an architectural or scope decision"
  "blocked|000000|Waiting on a dependency or prerequisite"
  "dependencies|0366d6|Pull requests that update dependency files"
  "rust|000000|Pull requests that update Rust code"
  "github_actions|000000|Pull requests that update GitHub Actions code"
  "skip-changelog|ffffff|Leave out of generated release notes"
)

if [ "$DRY_RUN" = true ]; then
  echo "=== DRY RUN (target repo: $REPO) ==="
  echo "No changes will be made to GitHub. Run with --apply to apply changes."
  echo ""
  for item in "${LABELS[@]}"; do
    IFS="|" read -r name color desc <<< "$item"
    printf "[dry-run] Label: %-22s #%-6s %s\n" "$name" "$color" "$desc"
  done
  echo ""
  echo "To create or update these labels, run:"
  echo "  $0 --apply --repo=$REPO"
  exit 0
fi

echo "Creating/updating labels on $REPO..."
for item in "${LABELS[@]}"; do
  IFS="|" read -r name color desc <<< "$item"
  echo "Applying: $name (#$color) - $desc"
  gh label create "$name" --description "$desc" --color "$color" --force --repo "$REPO"
done

echo "Labels successfully synced on $REPO."
