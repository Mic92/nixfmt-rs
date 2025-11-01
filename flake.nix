{
  description = "nixfmt-rs: Rust implementation of nixfmt";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    treefmt-nix.url = "github:numtide/treefmt-nix";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      imports = [ inputs.treefmt-nix.flakeModule ];

      perSystem =
        {
          pkgs,
          ...
        }:
        {
          # Development shell
          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              # Rust toolchain (rustup for custom toolchains)
              rustup
              ## Coverage tools
              cargo-tarpaulin
              #rustc
              #cargo
              rust-analyzer
              rustfmt
              # Development tools
              nixfmt # For comparing output
            ];

            shellHook = ''
              # Set up rustup home in project directory
              export RUSTUP_HOME="$PWD/.rustup"
              export CARGO_HOME="$PWD/.cargo"
              export PATH="$CARGO_HOME/bin:$PATH"
            '';
          };

          # treefmt configuration
          treefmt = {
            projectRootFile = "flake.nix";
            programs = {
              nixfmt.enable = true;
              rustfmt.enable = true;
            };
          };

          # Package output (will be filled in later phases)
          packages.default = pkgs.rustPlatform.buildRustPackage {
            pname = "nixfmt-rs";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
          };
        };
    };
}
