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
          plain = pkgs.callPackage ./nix/package.nix { };
          pgo = pkgs.callPackage ./nix/package-pgo.nix { nixpkgs-src = nixpkgs; };
        in
        {
          inherit plain pgo;
          # PGO needs to run the instrumented target binary during the build,
          # so fall back to the plain build when cross-compiling.
          default = if pkgs.stdenv.buildPlatform.canExecute pkgs.stdenv.hostPlatform then pgo else plain;
          wasm = pkgs.callPackage ./nix/wasm.nix { };
        }
      );

      devShells = forAllSystems (system: {
        default = pkgsFor.${system}.callPackage ./nix/shell.nix { };
        fuzz = pkgsFor.${system}.callPackage ./nix/fuzz-shell.nix { };
      });

      formatter = forAllSystems (system: treefmtEvalFor.${system}.config.build.wrapper);
      checks = forAllSystems (system: {
        formatting = treefmtEvalFor.${system}.config.build.check self;
      });
    };
}
