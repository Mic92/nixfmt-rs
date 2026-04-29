{ rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "nixfmt-rs";
  version = "0.1.0";
  src = ../.;
  cargoLock.lockFile = ../Cargo.lock;
}
