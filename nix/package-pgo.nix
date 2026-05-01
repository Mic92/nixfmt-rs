{
  callPackage,
  rustc,
  # Training corpus. The vendored fixtures alone are too small/synthetic to be
  # representative (a fixture-trained profile gave ~0% on real nixpkgs trees,
  # vs ~18% when trained on a few large real files). The flake passes its
  # locked nixpkgs source here so the profile is still a pure function of
  # locked inputs.
  nixpkgs-src,
}:
# Two-stage PGO build. The training run is a single-threaded walk over a
# fixed slice of the nixpkgs source tree plus the vendored fixtures, so the
# recorded branch/call counts are a pure function of (rustc, source,
# nixpkgs-src) and the result is as reproducible as the non-PGO build.
(callPackage ./package.nix { }).overrideAttrs (prev: {
  # llvm-profdata must match the LLVM rustc was built against.
  nativeBuildInputs = (prev.nativeBuildInputs or [ ]) ++ [ rustc.llvmPackages.llvm ];

  preBuild =
    # Stage 1–3 first so the RUSTFLAGS export from package.nix's preBuild
    # (path remap, -no_uuid) applies to the final stage-4 build below it.
    ''
      pgo=$PWD/pgo-data
      mkdir -p "$pgo"

      # Stage 1: instrumented build (separate target dir so the real build
      # below starts clean).
      env RUSTFLAGS="-Cprofile-generate=$pgo" \
        cargo build --release --frozen --offline -j $NIX_BUILD_CORES \
          --target-dir target/pgo-gen

      # Build scripts / proc macros executed above also dumped profraw; those
      # runs are not part of the workload and depend on build-time order.
      rm -f "$pgo"/*.profraw

      # Stage 2: train. One file per invocation, sorted order, single-threaded
      # parser → counts are fully determined by the inputs. nixpkgs/lib and
      # the three large files cover the size/shape distribution that the
      # synthetic fixtures alone miss.
      export LLVM_PROFILE_FILE="$pgo/%m.profraw"
      find tests/fixtures/nixfmt \
        ${nixpkgs-src}/lib \
        ${nixpkgs-src}/nixos/modules/services/networking/ssh/sshd.nix \
        ${nixpkgs-src}/pkgs/top-level/all-packages.nix \
        ${nixpkgs-src}/maintainers/maintainer-list.nix \
        -name '*.nix' | sort | while read -r f; do
        ./target/pgo-gen/release/nixfmt --check "$f" || true
      done
      unset LLVM_PROFILE_FILE

      # Stage 3: merge with a stable input order.
      llvm-profdata merge -o "$pgo/merged.profdata" \
        $(find "$pgo" -name '*.profraw' | sort)

      # Stage 4: the regular cargoBuildHook runs next and picks this up
      # (together with the remap / -no_uuid flags appended below).
      export RUSTFLAGS="''${RUSTFLAGS:-} -Cprofile-use=$pgo/merged.profdata"
    ''
    + prev.preBuild;
})
