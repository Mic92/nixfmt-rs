# Dev shell for cargo-fuzz. Uses the nixpkgs rustc with RUSTC_BOOTSTRAP=1
# instead of a separate nightly toolchain. nixpkgs rustc lacks the sanitizer
# runtimes, so cargo-fuzz must be run with `-s none`.
{
  lib,
  stdenv,
  mkShell,
  cargo,
  rustc,
  cargo-fuzz,
  libiconv,
}:
mkShell {
  packages = [
    cargo
    rustc
    cargo-fuzz
    # llvm-profdata / llvm-cov matching rustc's LLVM, for fuzz/coverage.sh.
    rustc.llvmPackages.llvm
  ]
  ++ lib.optionals stdenv.hostPlatform.isDarwin [ libiconv ];

  RUSTC_BOOTSTRAP = "1";

  shellHook = ''
    export CARGO_HOME="$PWD/.cargo"
    export PATH="$CARGO_HOME/bin:$PATH"
  ''
  + lib.optionalString stdenv.hostPlatform.isDarwin ''
    # libfuzzer-sys' build.rs needs -liconv on macOS.
    export LIBRARY_PATH="${libiconv}/lib''${LIBRARY_PATH:+:$LIBRARY_PATH}"
  '';
}
