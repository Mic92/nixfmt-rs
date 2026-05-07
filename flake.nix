{
  description = "nixfmt-rs: Rust implementation of nixfmt";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    treefmt-nix.url = "github:numtide/treefmt-nix";
  };

  outputs =
    {
      self,
      nixpkgs,
      treefmt-nix,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "riscv64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = forAllSystems (system: nixpkgs.legacyPackages.${system});
      treefmtEvalFor = forAllSystems (
        system: treefmt-nix.lib.evalModule pkgsFor.${system} (import ./nix/treefmt.nix)
      );
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor.${system};
        in
        rec {
          default = pkgs.callPackage ./nix/package.nix { };
        }
        # The rustc-wasm32 toolchain is unavailable on riscv64.
        // pkgs.lib.optionalAttrs (!pkgs.stdenv.hostPlatform.isRiscV64) {
          wasm = pkgs.callPackage ./nix/wasm.nix { };
        }
        // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          # Fully static musl binary for release artifacts.
          static = pkgs.pkgsStatic.callPackage ./nix/package.nix { };
        }
      );

      devShells = forAllSystems (system: {
        default = pkgsFor.${system}.callPackage ./nix/shell.nix { };
        fuzz = pkgsFor.${system}.callPackage ./nix/fuzz-shell.nix { };
      });

      formatter = forAllSystems (system: treefmtEvalFor.${system}.config.build.wrapper);
      checks = forAllSystems (
        system:
        # buildbot-nix builds .#checks; expose packages here so CI builds them.
        self.packages.${system}
        // {
          formatting = treefmtEvalFor.${system}.config.build.check self;
        }
      );
    };
}
