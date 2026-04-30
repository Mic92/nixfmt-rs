{
  mkShell,
  cargo,
  rustc,
  clippy,
  cargo-tarpaulin,
  rust-analyzer,
  rustfmt,
  nixfmt,
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
  ];

  shellHook = ''
    export CARGO_HOME="$PWD/.cargo"
    export PATH="$CARGO_HOME/bin:$PATH"
  '';
}
