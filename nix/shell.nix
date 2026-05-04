{
  mkShell,
  cargo,
  rustc,
  clippy,
  cargo-tarpaulin,
  rust-analyzer,
  rustfmt,
  callPackage,
  hyperfine,
  rsync,
}:
let
  nixfmt-rs = callPackage ./package.nix { };
in
mkShell {
  packages = [
    cargo
    rustc
    clippy
    cargo-tarpaulin
    rust-analyzer
    rustfmt
    # Patched reference for the parity test suite; see ./reference-nixfmt.nix.
    nixfmt-rs.referenceNixfmt
    # scripts/bench.sh
    hyperfine
    rsync
  ];

  shellHook = ''
    export CARGO_HOME="$PWD/.cargo"
    export PATH="$CARGO_HOME/bin:$PATH"
  '';
}
