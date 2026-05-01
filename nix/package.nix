{
  lib,
  stdenv,
  rustPlatform,
  nixfmt,
}:
rustPlatform.buildRustPackage {
  pname = "nixfmt-rs";
  version = "0.1.1";
  src = import ./source.nix { inherit lib; };
  cargoLock.lockFile = ../Cargo.lock;
  # The test suite shells out to the reference Haskell `nixfmt` to compare
  # output, so it must be on PATH during checkPhase. Pass `nixfmt = null`
  # (e.g. from pkgsStatic) to skip the suite where the reference can't build.
  doCheck = nixfmt != null;
  nativeCheckInputs = lib.optional (nixfmt != null) nixfmt;
  # The binary is named `nixfmt` (see Cargo.toml [[bin]]), not the pname.
  # Without this, lib.getExe guesses `nixfmt-rs` and treefmt-nix breaks.
  meta.mainProgram = "nixfmt";

  # Reproducibility: buildRustPackage does not yet remap $NIX_BUILD_TOP, so
  # panic-location strings from vendored crates leak the per-build sandbox
  # path (nix-<pid>-<rand>) into .rodata. ld64 also stamps a random LC_UUID.
  # Both make the Darwin binary non-reproducible regardless of PGO.
  preBuild = ''
    export RUSTFLAGS="''${RUSTFLAGS:-} --remap-path-prefix=$NIX_BUILD_TOP=/build"
    ${lib.optionalString stdenv.hostPlatform.isDarwin ''
      export RUSTFLAGS="$RUSTFLAGS -Clink-arg=-Wl,-no_uuid"
    ''}
  '';
}
