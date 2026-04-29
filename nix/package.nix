{ rustPlatform, nixfmt }:
rustPlatform.buildRustPackage {
  pname = "nixfmt-rs";
  version = "0.1.0";
  src = ../.;
  cargoLock.lockFile = ../Cargo.lock;
  # The test suite shells out to the reference Haskell `nixfmt` to compare
  # output, so it must be on PATH during checkPhase.
  nativeCheckInputs = [ nixfmt ];
}

