#!/usr/bin/env bash
# Run a cargo-fuzz target inside the nix fuzz shell.
# Usage: ./fuzz/fuzz.sh <target> [seconds] [extra-libfuzzer-args...]
# Example: ./fuzz/fuzz.sh fuzz_idempotent 120
set -euo pipefail

target="${1:?usage: fuzz.sh <target> [seconds] [extra-args...]}"
seconds="${2:-0}"
shift 2 2>/dev/null || shift $# 2>/dev/null

time_flag=()
if [ "$seconds" -gt 0 ] 2>/dev/null; then
  time_flag=(-max_total_time="$seconds")
fi

exec nix develop .#fuzz -c \
  cargo fuzz run "$target" -s none -- "${time_flag[@]}" "$@"
