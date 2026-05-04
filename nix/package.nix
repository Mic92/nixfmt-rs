{
  lib,
  stdenv,
  rustPlatform,
  scdoc,
  installShellFiles,
  versionCheckHook,
  git,
  nixfmt,
}:
let
  # The test suite diffs against a Haskell `nixfmt` carrying our upstream
  # idempotency patches; see ./reference-nixfmt.nix for rationale. Kept inside
  # this derivation so `callPackage ./nix/package.nix { }` works against any
  # nixpkgs without the caller having to know about the patch set.
  #
  # GHC has no bootstrap path on riscv64, so the Haskell reference cannot be
  # built there even though its meta.platforms claims otherwise. Fall back to
  # `null` (skipping the parity suite) rather than failing the whole build.
  referenceNixfmt =
    if nixfmt == null || stdenv.hostPlatform.isRiscV64 then
      null
    else
      import ./reference-nixfmt.nix { inherit nixfmt; };
in
rustPlatform.buildRustPackage {
  pname = "nixfmt-rs";
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).package.version;

  src = import ./source.nix { inherit lib; };
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    scdoc
    installShellFiles
  ];

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

  postBuild = ''
    scdoc < docs/nixfmt.1.scd > nixfmt.1
  '';

  # The test suite shells out to the reference Haskell `nixfmt` to compare
  # output, so it must be on PATH during checkPhase. Pass `nixfmt = null`
  # (e.g. from pkgsStatic) to skip the suite where the reference can't build.
  doCheck = referenceNixfmt != null;
  # `git` is required by the --mergetool tests.
  nativeCheckInputs = lib.optional (referenceNixfmt != null) referenceNixfmt ++ [ git ];

  postInstall = ''
    installManPage nixfmt.1
    installShellCompletion \
      --bash completions/nixfmt.bash \
      --zsh completions/_nixfmt \
      --fish completions/nixfmt.fish \
      --nushell completions/nixfmt.nu
  '';

  doInstallCheck = true;
  nativeInstallCheckInputs = [ versionCheckHook ];

  passthru = { inherit referenceNixfmt; };

  meta = {
    description = "Rust implementation of nixfmt with exact Haskell compatibility";
    homepage = "https://github.com/Mic92/nixfmt-rs";
    changelog = "https://github.com/Mic92/nixfmt-rs/releases";
    license = lib.licenses.mpl20;
    maintainers = with lib.maintainers; [ mic92 ];
    # The binary is named `nixfmt` (see Cargo.toml [[bin]]), not the pname.
    # Without this, lib.getExe guesses `nixfmt-rs` and treefmt-nix breaks.
    mainProgram = "nixfmt";
  };
}
