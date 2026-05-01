{
  projectRootFile = "flake.nix";
  programs = {
    nixfmt.enable = true;
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
