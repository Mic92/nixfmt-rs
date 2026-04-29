{
  projectRootFile = "flake.nix";
  programs = {
    nixfmt.enable = true;
    rustfmt.enable = true;
  };
  # Vendored upstream nixfmt golden inputs; must stay byte-identical.
  settings.global.excludes = [ "tests/fixtures/**" ];
}
