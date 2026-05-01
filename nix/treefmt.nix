{ pkgs, ... }:
{
  projectRootFile = "flake.nix";
  programs = {
    nixfmt = {
      enable = true;
      # Dogfood: format this repo with the binary built from this repo.
      package = pkgs.callPackage ./package.nix { };
    };
    rustfmt.enable = true;
  };
  settings.global.excludes = [
    # Vendored upstream nixfmt golden inputs; must stay byte-identical.
    "tests/fixtures/**"
    # Hand-written to exercise parser branches the fixtures miss; formatting
    # would normalise away the very constructs they target (bare URIs, ~, etc.).
    "fuzz/seeds/**"
    "fuzz/seeds-invalid/**"
  ];
}
