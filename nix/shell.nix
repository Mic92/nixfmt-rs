{
  mkShell,
  cargo,
  rustc,
  clippy,
  cargo-tarpaulin,
  rust-analyzer,
  rustfmt,
  nixfmt,
  hyperfine,
  rsync,
}:
mkShell {
  packages = [
    cargo
    rustc
    clippy
    cargo-tarpaulin
    rust-analyzer
    rustfmt
    nixfmt
    # scripts/bench.sh
    hyperfine
    rsync
  ];

  shellHook = ''
    export CARGO_HOME="$PWD/.cargo"
    export PATH="$CARGO_HOME/bin:$PATH"
  '';
}
