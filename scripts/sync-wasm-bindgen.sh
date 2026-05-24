#!/usr/bin/env bash
# Sync the wasm-bindgen crate pin in wasm/Cargo.toml with
# the wasm-bindgen-cli version from nixpkgs.
set -euo pipefail

cli=$(nix eval --inputs-from . --raw 'nixpkgs#wasm-bindgen-cli.version')
pinned=$(sed -n 's/^wasm-bindgen = "=\(.*\)"/\1/p' wasm/Cargo.toml)

if [ "$cli" = "$pinned" ]; then
  echo "wasm-bindgen already at $cli"
  exit 0
fi

echo "Updating wasm-bindgen: $pinned -> $cli"
sed -i "s/^wasm-bindgen = \"=.*\"/wasm-bindgen = \"=$cli\"/" wasm/Cargo.toml
cargo update -p wasm-bindgen --precise "$cli"
