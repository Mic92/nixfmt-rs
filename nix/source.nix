{ lib }:
let
  fs = lib.fileset;
in
fs.toSource {
  root = ../.;
  fileset = fs.unions [
    ../Cargo.toml
    ../Cargo.lock
    ../src
    ../benches
    ../tests
    # cargo workspace members; manifests must exist even when not built.
    ../wasm
    ../fuzz/Cargo.toml
    ../fuzz/fuzz_targets
  ];
}
