#!/usr/bin/env bash
# Full-nixpkgs differential sweep: compare ./target/release/nixfmt_rs against
# `nixfmt` on every *.nix under $NIXPKGS, in parallel.
#
#   MODE=format|ir|ast   what to compare (default: format)
#   JOBS=N               parallel workers (default: nproc)
#   LIMIT=N              cap file count (0 = all, default: 0)
#   MAX_BYTES=N          skip files larger than N bytes (0 = no cap)
#   REF_TIMEOUT=S        per-file timeout for the reference (ir/ast only)
#   NIXPKGS=path         nixpkgs checkout (default: ~/git/nixpkgs)
#   OUT=dir              output dir (default: ./sweep-out)
#
# Mismatches are written one-per-line to $OUT/mismatch-$MODE.txt with a
# DIFF/REJECT/TIMEOUT prefix.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
OURS=${OURS:-"$ROOT/target/release/nixfmt_rs"}
REF=${REF:-nixfmt}
MODE="${MODE:-${1:-format}}"

# Worker mode: re-entered via xargs. Kept free of `export -f` so it works
# regardless of which bash xargs picks up on macOS.
if [[ "${1:-}" == --worker ]]; then
  f=$2
  case "$MODE" in
  format)
    a=$("$REF" - <"$f" 2>/dev/null) || exit 0
    b=$(timeout 5 "$OURS" <"$f" 2>/dev/null) || {
      st=$?
      if [[ $st -eq 124 ]]; then echo "TIMEOUT $f"; else echo "REJECT $f"; fi
      exit 0
    }
    ;;
  ir | ast)
    # nixfmt --ast/--ir exits 1 even on success and writes the dump to stderr;
    # a real parse error mentions the file path, so use that to skip bad inputs.
    # Dumps can be multi-MB so compare via sha256 instead of shell vars.
    tmp=$(mktemp)
    trap 'rm -f "$tmp"' EXIT
    # nixfmt's --ast/--ir pretty-printer is O(scary) on large files; cap it.
    # nixfmt always exits 1 in these modes, so only treat the timeout sentinel
    # (124) as a skip.
    st=0
    timeout "${REF_TIMEOUT:-8}" "$REF" "--$MODE" "$f" 2>"$tmp" >/dev/null || st=$?
    [[ $st -ne 124 ]] || exit 0
    head1=$(head -n1 "$tmp")
    [[ -n "$head1" && "$head1" != *"$f"* ]] || exit 0
    a=$(sha256sum <"$tmp")
    b=$(timeout 10 "$OURS" "--$MODE" <"$f" 2>/dev/null | sha256sum)
    st=${PIPESTATUS[0]}
    if [[ $st -ne 0 ]]; then
      if [[ $st -eq 124 ]]; then echo "TIMEOUT $f"; else echo "REJECT $f"; fi
      exit 0
    fi
    ;;
  *)
    echo "unknown MODE=$MODE" >&2
    exit 2
    ;;
  esac
  [[ "$a" == "$b" ]] || echo "DIFF $f"
  exit 0
fi

NIXPKGS="${NIXPKGS:-$HOME/git/nixpkgs}"
JOBS="${JOBS:-$(nproc)}"
LIMIT="${LIMIT:-0}"
OUT="${OUT:-$ROOT/sweep-out}"

mkdir -p "$OUT"
LIST="$OUT/files.txt"
RESULT="$OUT/mismatch-$MODE.txt"
: >"$RESULT"

MAX_BYTES="${MAX_BYTES:-0}"
if [[ "$MAX_BYTES" -gt 0 ]]; then
  find "$NIXPKGS" -name '*.nix' -type f -size -"${MAX_BYTES}"c | sort >"$LIST"
else
  find "$NIXPKGS" -name '*.nix' -type f | sort >"$LIST"
fi
if [[ "$LIMIT" -gt 0 ]]; then
  head -n "$LIMIT" "$LIST" >"$LIST.tmp" && mv "$LIST.tmp" "$LIST"
fi
TOTAL=$(wc -l <"$LIST")
echo "sweeping $TOTAL files mode=$MODE jobs=$JOBS" >&2

export OURS REF MODE
xargs -a "$LIST" -P "$JOBS" -n 1 -d '\n' "$BASH" "$0" --worker >"$RESULT"

sort -o "$RESULT" "$RESULT"
echo "mismatches ($MODE): $(wc -l <"$RESULT") -> $RESULT" >&2
for k in DIFF REJECT TIMEOUT; do
  printf '  %-7s %s\n' "$k" "$(grep -c "^$k " "$RESULT" || true)" >&2
done
