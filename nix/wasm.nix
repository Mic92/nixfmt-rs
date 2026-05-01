{
  lib,
  rustPlatform,
  rustc,
  cargo,
  wasm-bindgen-cli,
  binaryen,
  lld,
}:
rustPlatform.buildRustPackage {
  pname = "nixfmt-wasm";
  version = "0.1.0";
  src = import ./source.nix { inherit lib; };
  cargoLock.lockFile = ../Cargo.lock;

  # cargoBuildHook hard-codes the host target via env+arg, so override
  # buildPhase entirely rather than fight it for a wasm32 cross-build.
  dontCargoBuild = true;
  doCheck = false;

  nativeBuildInputs = [
    rustc
    cargo
    wasm-bindgen-cli
    binaryen
    lld
  ];

  buildPhase = ''
    runHook preBuild
    export CARGO_TARGET_DIR="$PWD/target"
    cargo build \
      --release \
      --offline \
      --manifest-path wasm/Cargo.toml \
      --lib \
      --target wasm32-unknown-unknown \
      -j "$NIX_BUILD_CORES"
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p $out/pkg
    wasm-bindgen \
      --target web \
      --out-dir $out/pkg \
      target/wasm32-unknown-unknown/release/nixfmt_wasm.wasm
    wasm-opt -Oz \
      $out/pkg/nixfmt_wasm_bg.wasm \
      -o $out/pkg/nixfmt_wasm_bg.wasm.opt
    mv $out/pkg/nixfmt_wasm_bg.wasm.opt $out/pkg/nixfmt_wasm_bg.wasm
    runHook postInstall
  '';

  meta = with lib; {
    description = "WebAssembly build of nixfmt-rs for browser embedding";
    license = licenses.mpl20;
    platforms = platforms.unix;
  };
}
