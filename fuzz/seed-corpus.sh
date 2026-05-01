#!/usr/bin/env bash
# Populate fuzz/corpus/<target>/ from the vendored nixfmt fixture set plus the
# hand-written fuzz/seeds/ that cover constructs the fixtures lack, so the
# fuzzer starts from realistic, parseable inputs instead of an empty corpus.
set -euo pipefail
cd "$(dirname "$0")/.."

targets=(fuzz_parse fuzz_roundtrip fuzz_idempotent fuzz_debug_dumps)
for target in "${targets[@]}"; do
	mkdir -p "fuzz/corpus/$target"
done

find tests/fixtures/nixfmt fuzz/seeds -name '*.nix' -print0 |
	while IFS= read -r -d '' f; do
		name=${f//\//_}
		for target in "${targets[@]}"; do
			cp "$f" "fuzz/corpus/$target/$name"
		done
	done

echo "seeded $(find fuzz/corpus/fuzz_roundtrip -type f | wc -l | tr -d ' ') files per target"
