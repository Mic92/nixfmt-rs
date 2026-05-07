{ pkgs, ... }:
{
  projectRootFile = "flake.nix";
  programs = {
    nixfmt = {
      enable = true;
      # TODO: drop the override once nixpkgs' nixfmt-rs no longer pulls the
      # Haskell reference into checkPhase (breaks eval on riscv64).
      package = (pkgs.nixfmt-rs.override { nixfmt = null; }).overrideAttrs {
        doCheck = false;
      };
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
