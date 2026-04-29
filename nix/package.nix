{ rustPlatform, nixfmt }:
rustPlatform.buildRustPackage {
  pname = "nixfmt-rs";
  version = "0.1.0";
  src = ../.;
  cargoLock.lockFile = ../Cargo.lock;
  # The test suite shells out to the reference Haskell `nixfmt` to compare
  # output, so it must be on PATH during checkPhase.
  nativeCheckInputs = [ nixfmt ];
  # The binary is named `nixfmt` (see Cargo.toml [[bin]]), not the pname.
  # Without this, lib.getExe guesses `nixfmt-rs` and treefmt-nix breaks.
  meta.mainProgram = "nixfmt";
}
