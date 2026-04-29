#!/usr/bin/env bash
# Populate fuzz/corpus/<target>/ from the vendored nixfmt fixture set so the
# fuzzer starts from realistic, parseable inputs instead of an empty corpus.
set -euo pipefail
cd "$(dirname "$0")/.."

for target in fuzz_parse fuzz_roundtrip fuzz_idempotent; do
	mkdir -p "fuzz/corpus/$target"
done

find tests/fixtures/nixfmt -name '*.nix' -print0 |
	while IFS= read -r -d '' f; do
		name=${f//\//_}
		for target in fuzz_parse fuzz_roundtrip fuzz_idempotent; do
			cp "$f" "fuzz/corpus/$target/$name"
		done
	done

echo "seeded $(find fuzz/corpus/fuzz_roundtrip -type f | wc -l | tr -d ' ') files per target"
