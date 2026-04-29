#!/usr/bin/env bash
# Differential sweep: compare ./target/release/nixfmt_rs against `nixfmt`
# on a bounded sample of nixpkgs *.nix files. Records mismatches per mode.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
OURS="$ROOT/target/release/nixfmt_rs"
REF=nixfmt
NIXPKGS="${NIXPKGS:-$HOME/git/nixpkgs}"
LIMIT="${LIMIT:-2000}"
OUT="${OUT:-$ROOT/sweep-out}"
MODE="${1:-format}" # format | ir | ast

mkdir -p "$OUT"
: >"$OUT/mismatch-$MODE.txt"

mapfile -t files < <(find "$NIXPKGS/pkgs" -name '*.nix' -type f | sort | head -n "$LIMIT")
echo "sweeping ${#files[@]} files in mode=$MODE" >&2

i=0
for f in "${files[@]}"; do
  i=$((i + 1))
  case "$MODE" in
  format)
    a=$("$REF" - <"$f" 2>/dev/null) || continue
    b=$(timeout 5 "$OURS" <"$f" 2>/dev/null) || {
      st=$?
      if [[ $st -eq 124 ]]; then
        echo "TIMEOUT $f" >>"$OUT/mismatch-$MODE.txt"
      else
        echo "REJECT $f" >>"$OUT/mismatch-$MODE.txt"
      fi
      continue
    }
    ;;
  ir | ast)
    # nixfmt --ast/--ir exits 1 even on success and writes the dump to stderr;
    # a real parse error mentions the file path, so use that to skip bad inputs.
    a=$("$REF" "--$MODE" "$f" 2>&1 >/dev/null) || true
    [[ -n "$a" && "$a" != *"$f"* ]] || continue
    b=$("$OURS" "--$MODE" <"$f" 2>/dev/null) || {
      echo "REJECT $f" >>"$OUT/mismatch-$MODE.txt"
      continue
    }
    ;;
  esac
  if [[ "$a" != "$b" ]]; then
    echo "DIFF $f" >>"$OUT/mismatch-$MODE.txt"
  fi
  if ((i % 200 == 0)); then echo "  $i/${#files[@]}" >&2; fi
done

echo "mismatches ($MODE): $(wc -l <"$OUT/mismatch-$MODE.txt")" >&2
