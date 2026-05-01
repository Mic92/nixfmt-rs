#!/usr/bin/env bash
# Print per-file coverage of one or more fuzz targets over their corpora.
# Run inside `nix develop .#fuzz`.
#
# usage: fuzz/coverage.sh <target> [<target>...] [-- <extra llvm-cov args>]
#        fuzz/coverage.sh all
set -euo pipefail
cd "$(dirname "$0")/.."

targets=()
cov_args=()
while [[ $# -gt 0 ]]; do
	case "$1" in
	--)
		shift
		cov_args=("$@")
		break
		;;
	all) targets=(fuzz_parse fuzz_roundtrip fuzz_idempotent fuzz_debug_dumps) ;;
	*) targets+=("$1") ;;
	esac
	shift
done
[[ ${#targets[@]} -gt 0 ]] || {
	echo "usage: fuzz/coverage.sh <target> [<target>...] | all" >&2
	exit 1
}

triple=$(rustc -vV | sed -n 's/^host: //p')
ignore='(\.cargo/registry|/nix/store|/rustc/)'
objects=()
profiles=()

for target in "${targets[@]}"; do
	# cargo-fuzz's own merge step hardcodes llvm-profdata to a sysroot path that
	# nixpkgs rustc does not ship; tolerate that failure and merge below.
	cargo fuzz coverage -s none "$target" || true
	raw="fuzz/coverage/$target/raw"
	shopt -s nullglob
	t_prof=("$raw"/*.profraw)
	if [[ ${#t_prof[@]} -eq 0 ]]; then
		echo "error: no .profraw files in $raw (build or run failed above)" >&2
		exit 1
	fi
	profiles+=("${t_prof[@]}")
	objects+=(--object "target/$triple/coverage/$triple/release/$target")
done

if [[ ${#targets[@]} -eq 1 ]]; then
	prof="fuzz/coverage/${targets[0]}/coverage.profdata"
else
	mkdir -p fuzz/coverage/combined
	prof="fuzz/coverage/combined/coverage.profdata"
fi

llvm-profdata merge -sparse "${profiles[@]}" -o "$prof"

llvm-cov report \
	--instr-profile "$prof" \
	"${objects[@]}" \
	--ignore-filename-regex "$ignore" \
	"${cov_args[@]}"

echo
echo "HTML report:"
echo "  llvm-cov show --format=html --instr-profile $prof ${objects[*]} --ignore-filename-regex '$ignore' -o fuzz/coverage/html"
