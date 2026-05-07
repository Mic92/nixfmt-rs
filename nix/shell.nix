{
  mkShell,
  cargo,
  rustc,
  clippy,
  cargo-tarpaulin,
  cargo-mutants,
  cargo-insta,
  rust-analyzer,
  rustfmt,
  hyperfine,
  rsync,
}:
mkShell {
  packages = [
    cargo
    rustc
    clippy
    cargo-tarpaulin
    cargo-mutants
    cargo-insta
    rust-analyzer
    rustfmt
    # scripts/bench.sh
    hyperfine
    rsync
  ];

  shellHook = ''
    export CARGO_HOME="$PWD/.cargo"
    export PATH="$CARGO_HOME/bin:$PATH"
  '';
}
