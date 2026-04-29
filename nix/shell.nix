{
  mkShell,
  rustup,
  cargo-tarpaulin,
  rust-analyzer,
  rustfmt,
  nixfmt,
}:
mkShell {
  packages = [
    rustup
    cargo-tarpaulin
    rust-analyzer
    rustfmt
    nixfmt
  ];

  shellHook = ''
    # Set up rustup home in project directory
    export RUSTUP_HOME="$PWD/.rustup"
    export CARGO_HOME="$PWD/.cargo"
    export PATH="$CARGO_HOME/bin:$PATH"
  '';
}
