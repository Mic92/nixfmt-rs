nixfmt(1) "nixfmt-rs" "General Commands Manual"

# NAME

nixfmt - format Nix source code (Rust implementation)

# SYNOPSIS

*nixfmt* [_OPTIONS_] [_FILES_...|*-*]

# DESCRIPTION

*nixfmt* reformats Nix expressions to a canonical style. It is a Rust
reimplementation of the reference Haskell *nixfmt*(1) and aims for byte-for-byte
output parity on supported inputs.

When invoked with one or more _FILES_, each file is reformatted in place. If a
_FILE_ is a directory, *nixfmt* recurses into it and formats every *.nix* file
it finds. With no _FILES_, or with the single argument *-*, *nixfmt* reads the
source from standard input and writes the formatted result to standard output.
Bare invocation (no _FILES_, no *-*) is deprecated; use *-* to make the intent
explicit.

# OPTIONS

*-w*, *--width*=_INT_
	Maximum line width in characters. Default: *100*.

*--indent*=_INT_
	Number of spaces to use for one indentation level. Default: *2*.

*-c*, *--check*
	Do not modify any files; instead exit with status *1* if any input is not
	already formatted. In stdin mode no output is produced on standard output.

*-m*, *--mergetool*
	Run as a *git mergetool*(1). Expects exactly four file arguments
	_BASE_ _LOCAL_ _REMOTE_ _MERGED_. Each of _BASE_, _LOCAL_ and _REMOTE_ is
	pre-formatted, *git merge-file*(1) is invoked on them, the result is
	formatted, and finally moved onto _MERGED_. The exit status of
	*git merge-file* is forwarded so the conflict count is preserved. Skipped
	with a warning when _MERGED_ is not a *.nix* file.

*-q*, *--quiet*
	Suppress non-fatal diagnostics on standard error.

*-s*, *--strict*
	Accepted for compatibility with the reference *nixfmt*; currently a no-op.

*-v*, *--verify*
	After formatting, reparse and reformat the output to ensure idempotence.
	Fails with a non-zero exit status if the second pass differs from the first.

*-a*, *--ast*
	Print the internal AST to standard error (debugging aid). Exits with
	status *1*.

*--ir*
	Print the internal intermediate representation to standard error
	(debugging aid). Exits with status *1*.

*-f*, *--filename*=_NAME_
	Filename to use in diagnostic messages when input is read from standard
	input. Useful for editor integrations.

*--message-format*=_FMT_
	How diagnostics are rendered on standard error. _FMT_ is one of:

	*human* (default)
		rustc-style snippets with carets and fix-it hints.

	*json*
		One JSON object per line, shaped like an LSP _Diagnostic_: _file_,
		_severity_, _code_, _message_, 0-based _range_, _byteRange_, optional
		_help_ and _relatedInformation_, plus the human _rendered_ text. All
		standard-error output (including *--check* results and I/O errors) is
		JSON in this mode.

*-?*, *--help*
	Display a short help message and exit.

*-V*, *--version*
	Print the program version and exit.

*--numeric-version*
	Print just the numeric version (no program name) and exit.

*--*
	End of options. All remaining arguments are treated as file paths.

# EXIT STATUS

*0*
	All inputs were processed (and, in *--check* mode, all were already
	formatted).

*1*
	A parse error, an unformatted input in *--check* mode, an I/O error, or an
	invalid invocation.

In *--mergetool* mode the exit status is the exit code of *git merge-file*,
which is the number of remaining merge conflicts (or *0* if none).

# EXAMPLES

Format every *.nix* file under the current directory in place:

	*nixfmt .*

Check formatting in CI without modifying files:

	*nixfmt --check .*

Format a snippet from a pipeline:

	*echo '{a=1;}' | nixfmt -*

Consume diagnostics from a wrapper or LSP:

	*nixfmt --message-format=json --check . 2>&1 >/dev/null | jq .*

Use as a *git mergetool*:

	*git config mergetool.nixfmt.cmd 'nixfmt --mergetool "$BASE" "$LOCAL" "$REMOTE" "$MERGED"'*

# SEE ALSO

*nix*(1), *git-mergetool*(1), *git-merge-file*(1)

The reference Haskell implementation lives at
<https://github.com/NixOS/nixfmt>. The Rust port lives at
<https://github.com/Mic92/nixfmt-rs>.
