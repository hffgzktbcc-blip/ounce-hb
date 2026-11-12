#!/usr/bin/env bash
# Fails on (1) binary files that are not in docs/PROVENANCE.md and (2) code that handles
# console keys or decrypts retail containers. Run from anywhere; operates on the repo root.
# Self-test: scripts/guardrails.sh --self-test
set -u
cd "$(dirname "$0")/.." || exit 2
ROOT=$(pwd)
REGISTER=docs/PROVENANCE.md
fail=0

# Text-like extensions that are scanned for forbidden code patterns.
CODE_GLOB='\.(rs|toml|py|sh|c|h|cc|cpp|hpp|js|ts|go|java|swift|rb|yml|yaml|json|cmake|mk|s|asm)$'
# Extensions that are always refused, even if registered.
DENY_EXT='\.(nsp|xci|nca|keys|firmware)$'
# Case-insensitive extended regex of forbidden code patterns.
PATTERNS='prod[._ -]?keys|title[._ -]?keys|(nca|xci|ncz|nsp)[a-z0-9_ ./-]{0,30}decrypt|decrypt[a-z0-9_ ./-]{0,30}(nca|xci|ncz|nsp)|header_key|key_area_key|master_key|keyblob|aes[-_ ]?xts|aes[-_ ]?128[-_ ]?(ctr|ecb|xts)'

list_files() {
  { git ls-files; git ls-files --others --exclude-standard; } 2>/dev/null | sort -u
}

check_binaries() {
  while IFS= read -r f; do
    [ -f "$f" ] || continue
    [ -s "$f" ] || continue
    if echo "$f" | grep -Eiq "$DENY_EXT"; then
      echo "FORBIDDEN file type: $f"; fail=1; continue
    fi
    if LC_ALL=C grep -qI . "$f"; then continue; fi   # text
    if ! grep -Fq "| \`$f\`" "$REGISTER"; then
      echo "UNREGISTERED binary (add to $REGISTER): $f"; fail=1
    fi
  done < <(list_files)
}

check_patterns() {
  while IFS= read -r f; do
    [ -f "$f" ] || continue
    case "$f" in scripts/guardrails.sh|scripts/test_guardrails.sh) continue ;; esac
    echo "$f" | grep -Eiq "$CODE_GLOB" || continue
    if hits=$(LC_ALL=C grep -nEi "$PATTERNS" "$f"); then
      echo "FORBIDDEN key/decryption pattern in $f:"; echo "$hits" | sed 's/^/    /'; fail=1
    fi
  done < <(list_files)
}

self_test() {
  tmp=$(mktemp -d) || exit 2
  trap 'rm -rf "$tmp"' EXIT
  mkdir -p "$tmp/scripts" "$tmp/docs"
  cp "$ROOT/scripts/guardrails.sh" "$tmp/scripts/"; cp "$ROOT/$REGISTER" "$tmp/docs/"
  ( cd "$tmp" && git init -q . && printf 'fn main() {}\n' > ok.rs && git add -A )
  "$tmp/scripts/guardrails.sh" >/dev/null || { echo "self-test: clean tree should pass"; exit 1; }
  printf '\x00\x01\x02' > "$tmp/blob.bin"
  "$tmp/scripts/guardrails.sh" >/dev/null && { echo "self-test: unregistered binary should fail"; exit 1; }
  rm "$tmp/blob.bin"
  printf 'let k = load("prod.keys");\n' > "$tmp/bad.rs"
  "$tmp/scripts/guardrails.sh" >/dev/null && { echo "self-test: key pattern should fail"; exit 1; }
  printf 'fn decrypt_nca() {}\n' > "$tmp/bad.rs"
  "$tmp/scripts/guardrails.sh" >/dev/null && { echo "self-test: NCA decryption should fail"; exit 1; }
  echo "guardrails self-test: ok"
}

if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi

check_binaries
check_patterns
if [ "$fail" -ne 0 ]; then echo "guardrails: FAILED"; exit 1; fi
echo "guardrails: ok"
