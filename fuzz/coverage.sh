#!/usr/bin/env bash
# Print per-file coverage of a fuzz target over its corpus.
# Run inside `nix develop .#fuzz`.
set -euo pipefail
cd "$(dirname "$0")/.."

target=${1:?usage: fuzz/coverage.sh <fuzz_target> [extra llvm-cov args...]}
shift

triple=$(rustc -vV | sed -n 's/^host: //p')
bin="target/$triple/coverage/$triple/release/$target"
raw="fuzz/coverage/$target/raw"
prof="fuzz/coverage/$target/coverage.profdata"
ignore='(\.cargo/registry|/nix/store|/rustc/)'

# cargo-fuzz's own merge step hardcodes llvm-profdata to a sysroot path that
# nixpkgs rustc does not ship; tolerate that failure and merge below.
cargo fuzz coverage -s none "$target" || true

shopt -s nullglob
profiles=("$raw"/*.profraw)
if [[ ${#profiles[@]} -eq 0 ]]; then
	echo "error: no .profraw files in $raw (build or run failed above)" >&2
	exit 1
fi

llvm-profdata merge -sparse "${profiles[@]}" -o "$prof"

llvm-cov report \
	--instr-profile "$prof" \
	--object "$bin" \
	--ignore-filename-regex "$ignore" \
	"$@"

echo
echo "HTML report:"
echo "  llvm-cov show --format=html --instr-profile $prof --object $bin --ignore-filename-regex '$ignore' -o fuzz/coverage/$target/html"
