{
  lib,
  rustPlatform,
  rustc,
  cargo,
  wasm-bindgen-cli,
  binaryen,
  lld,
  jq,
}:
let
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).package.version;
in
rustPlatform.buildRustPackage {
  pname = "nixfmt-wasm";
  inherit version;
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
    jq
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
    raw=target/wasm32-unknown-unknown/release/nixfmt_wasm.wasm

    # Playground (kept at $out/pkg for backwards compat with gh-pages.yml).
    wasm-bindgen --target web --out-dir $out/pkg $raw
    wasm-opt -Oz $out/pkg/nixfmt_wasm_bg.wasm -o $out/pkg/nixfmt_wasm_bg.wasm

    # npm package: bundler + node + web entry points under one package.json.
    npm=$out/npm
    for t in bundler nodejs web; do
      dir=$npm/''${t/nodejs/node}
      wasm-bindgen --target $t --out-dir $dir $raw
      wasm-opt -Oz $dir/nixfmt_wasm_bg.wasm -o $dir/nixfmt_wasm_bg.wasm
    done
    # node/ is CommonJS; override the package-level "type":"module".
    echo '{"type":"commonjs"}' > $npm/node/package.json
    jq '.version = "${version}"' ${../wasm/package.json} > $npm/package.json
    cp ${../wasm/README.md} $npm/README.md
    cp ${../LICENSE} $npm/LICENSE

    runHook postInstall
  '';

  meta = with lib; {
    description = "WebAssembly build of nixfmt-rs for browser embedding";
    license = licenses.mpl20;
    platforms = platforms.unix;
  };
}
