#!/usr/bin/env nix
#!nix develop ..# --command bash
# shellcheck shell=bash
#
# Reproduce the README benchmark table: nixfmt-rs vs nixfmt (Haskell) vs
# nixfmt-tree over a nixpkgs checkout.
#
# Usage: scripts/bench.sh [NIXPKGS_DIR]
#
# With no argument, the nixpkgs revision pinned in this flake's lock file
# is used as the formatting corpus, so the benchmark is fully reproducible.
#
# Env: RUNS (default 3), WARMUP (default 1)

set -euo pipefail
cd "$(dirname "$0")/.."

RUNS=${RUNS:-3}
WARMUP=${WARMUP:-1}

if [[ $# -ge 1 ]]; then
  [[ -d "$1" ]] || {
    echo "error: not a directory: $1" >&2
    exit 1
  }
  NIXPKGS=$1
else
  echo ">> fetching pinned nixpkgs source from flake.lock"
  NIXPKGS=$(nix eval --raw --inputs-from . nixpkgs#path)
fi

echo ">> building nixfmt-rs (release)"
cargo build --quiet --release
RS=$PWD/target/release/nixfmt

echo ">> resolving reference binaries from this flake's pinned nixpkgs"
build() { nix build --no-link --print-out-paths --inputs-from . "nixpkgs#$1"; }
HS=$(build nixfmt)/bin/nixfmt
TREE=$(build nixfmt-tree)/bin/treefmt
TREEFMT=$(build treefmt)/bin/treefmt

# treefmt config that drives nixfmt-rs instead of the Haskell binary.
TF_RS=$(mktemp -t treefmt-rs.XXXXXX.toml)
trap 'rm -f "$TF_RS"' EXIT
cat >"$TF_RS" <<EOF
[formatter.nixfmt]
command = "$RS"
includes = ["*.nix"]
EOF

NFILES=$(find "$NIXPKGS" -name '*.nix' | wc -l | tr -d ' ')
echo
echo "nixpkgs:    $NIXPKGS ($NFILES .nix files)"
"$HS" --version
"$RS" --version
echo

# Single large file (pure formatter throughput, no fs walk). --check parses and
# formats but does not write, so the read-only store path is fine and both
# tools do the same work.
BIG=$NIXPKGS/pkgs/top-level/all-packages.nix
echo "== single file: $(wc -l <"$BIG") lines =="
hyperfine --warmup 3 --runs 15 -N \
  -n nixfmt-hs "$HS --check $BIG" \
  -n nixfmt-rs "$RS --check $BIG"

echo
echo "== full tree: --check $NIXPKGS =="
# treefmt's --fail-on-change still writes; route it through a tmpfs copy so the
# user's checkout is not touched.
WORK=$(mktemp -d -t nixfmt-bench.XXXXXX)
trap 'chmod -R u+w "$WORK" 2>/dev/null; rm -rf "$WORK" "$TF_RS"' EXIT
echo ">> rsyncing nixpkgs to $WORK (treefmt writes in place)"
rsync -a --delete --exclude .git "$NIXPKGS"/ "$WORK"/
# When NIXPKGS is the flake-locked store path the rsync inherits its read-only
# bits, which breaks `git init`, treefmt's in-place writes and the cleanup trap.
chmod -R u+w "$WORK"
git -C "$WORK" init -q && git -C "$WORK" add -A -f >/dev/null && git -C "$WORK" commit -q -m bench --no-gpg-sign

hyperfine -i --warmup "$WARMUP" --runs "$RUNS" \
  --prepare "git -C $WORK checkout -q -- ." \
  --export-markdown bench.md \
  -n "nixfmt-rs --check" "$RS --check $WORK" \
  -n "treefmt + nixfmt-rs" "$TREEFMT --config-file $TF_RS --no-cache --fail-on-change --tree-root $WORK" \
  -n "nixfmt-tree (hs)" "$TREE --no-cache --fail-on-change --tree-root $WORK" \
  -n "nixfmt-hs --check" "$HS --check $WORK"

echo
echo ">> markdown table written to ./bench.md"
