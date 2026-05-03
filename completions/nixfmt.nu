export extern nixfmt [
  ...files: path                          # Nix files or directories to format
  --width(-w): int                        # Maximum width in characters
  --indent: int                           # Number of spaces for indentation
  --check(-c)                             # Check whether files are formatted
  --mergetool(-m)                         # Git mergetool mode
  --quiet(-q)                             # Do not report errors
  --strict(-s)                            # Enable stricter formatting mode
  --verify(-v)                            # Sanity-check output after formatting
  --ast(-a)                               # Dump internal AST to stderr
  --ir                                    # Dump internal IR to stderr
  --filename(-f): string                  # Display name for stdin input
  --message-format: string@"nu-complete nixfmt message-format"  # Diagnostic output format
  --help(-h)                              # Show help
  --version(-V)                           # Print version
  --numeric-version                       # Print just the version number
]

def "nu-complete nixfmt message-format" [] { [human json] }
