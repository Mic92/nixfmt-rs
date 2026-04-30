//! PrettySimple trait for formatting AST and IR nodes to match nixfmt Haskell's output
//!
//! This implementation is based on the pretty-simple Haskell library:
//! <https://github.com/cdepillabout/pretty-simple>
//!
//! Key algorithm from pretty-simple's `list` function:
//! - Empty list: []
//! - Single simple element: [ element ]
//! - Otherwise: multiline with comma-first

mod ast;
mod ir;

use std::fmt::Debug;

/// Writer interface - handles output, colors, and indentation
pub trait Writer {
    /// Write plain text at current indentation
    fn write_plain(&mut self, text: &str);

    /// Write colored text at current indentation
    fn write_colored(&mut self, text: &str, color: &str);

    /// Start a new line
    fn newline(&mut self);

    /// Execute a closure with increased delimiter color depth
    fn with_color<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R;

    /// Get the current delimiter color (only valid within `with_color`)
    fn current_color(&self) -> &'static str;

    /// Execute a closure with increased depth
    fn with_depth<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R;

    /// Get the source text (used for computing line numbers from byte offsets)
    fn source(&self) -> &str;
}

// ANSI color codes
pub(crate) const NUMBER_COLOR: &str = "\x1b[0;92;1m";
pub(crate) const STRING_QUOTE_COLOR: &str = "\x1b[0;97;1m";
pub(crate) const STRING_CONTENT_COLOR: &str = "\x1b[0;94;1m";

/// Trait for types that can be formatted as Haskell-style output
pub trait PrettySimple: Debug {
    fn format<W: Writer>(&self, w: &mut W);

    /// Check if this value is "simple" (can be formatted inline)
    /// Based on pretty-simple's isSimple function
    fn is_simple(&self) -> bool {
        false
    }

    /// Check if this type has built-in delimiters (brackets, braces, parens)
    /// Types with delimiters don't need extra parens when used as constructor arguments
    fn has_delimiters(&self) -> bool {
        false
    }

    /// Whether this value should be wrapped in parentheses on a single line when used as an argument.
    /// Pretty-simple prints certain constructor arguments (like `Spacing ( Newlines n )`) using inline parens,
    /// which differs from both the simple and the delimiter cases.
    fn renders_inline_parens(&self) -> bool {
        false
    }

    /// Whether this value is logically empty (used for collection heuristics)
    fn is_empty(&self) -> bool {
        false
    }

    /// Check if this represents a single atomic element in Haskell's parsed form
    /// True for: primitives (String/usize/bool), nullary constructors (EmptyLine)
    /// False for: constructor applications (TextPart/LineComment), delimited types (Vec/Ann)
    ///
    /// This is used by Vec::is_simple() to determine if a single-element Vec is structurally simple.
    /// In Haskell, "TextPart \"hello\"" parses to [Other "TextPart ", StringLit "hello"] (2 elements),
    /// so Vec<StringPart> with [TextPart] is NOT structurally simple, even though TextPart is simple for rendering.
    fn is_atomic(&self) -> bool {
        false
    }
}

/// Escape non-printable characters in strings to match Haskell's isPrint behavior
/// This ensures control characters and format characters are displayed as escape
/// sequences rather than being interpreted by the terminal or being invisible.
///
/// Matches pretty-simple's escapeNonPrintable function which uses:
/// - Haskell's isPrint to determine what to escape
/// - \xH format (minimal hex, no leading zeros) for escaped characters
/// - Allows newlines to pass through (they're handled separately)
///
/// Note: The parser already keeps escape sequences like \n, \r, \t as literal
/// backslash+char in the AST. We only need to escape actual control/format characters
/// (like ESC 0x1b or zero-width space U+200B) that appear as literal bytes.
///
/// Returns a Cow to avoid allocation when no escaping is needed.
pub(crate) fn escape_string(s: &str) -> std::borrow::Cow<'_, str> {
    // Helper: Check if a character is non-printable (matches Haskell's not isPrint)
    // Haskell's isPrint returns False for Unicode categories: Cc, Cf, Cs, Co, Cn, Zl, Zp
    // Source: https://hackage.haskell.org/package/base-4.21.0.0/docs/Data-Char.html#v:isPrint
    fn is_non_printable(ch: char) -> bool {
        let code = ch as u32;

        // Control characters (Cc category) - except newline which we allow
        if ch.is_control() && ch != '\n' {
            return true;
        }

        // Line and Paragraph Separators (Zl, Zp categories)
        if matches!(code, 0x2028 | 0x2029) {
            return true;
        }

        // Surrogates (Cs) are not valid in Rust char, so we don't need to check for them

        // Format characters (Cf category) - complete list from Unicode Character Database
        // Source: https://www.compart.com/en/unicode/category/Cf (161 characters)
        matches!(code,
            0x00AD |               // SOFT HYPHEN
            0x0600..=0x0605 |      // Arabic Number signs
            0x061C |               // ARABIC LETTER MARK
            0x06DD |               // ARABIC END OF AYAH
            0x070F |               // SYRIAC ABBREVIATION MARK
            0x08E2 |               // ARABIC DISPUTED END OF AYAH
            0x180E |               // MONGOLIAN VOWEL SEPARATOR
            0x200B..=0x200F |      // Zero-width space, joiners, marks
            0x202A..=0x202E |      // Bidirectional formatting
            0x2060..=0x2064 |      // Word joiner, invisible operators
            0x2066..=0x206F |      // Bidirectional isolates and deprecated
            0xFEFF |               // ZERO WIDTH NO-BREAK SPACE (BOM)
            0xFFF9..=0xFFFB |      // Interlinear annotation
            0x110BD |              // KAITHI NUMBER SIGN
            0x110CD |              // KAITHI NUMBER SIGN ABOVE
            0x13430..=0x13438 |    // EGYPTIAN HIEROGLYPH format controls
            0x1BCA0..=0x1BCA3 |    // SHORTHAND FORMAT controls
            0x1D173..=0x1D17A |    // MUSICAL SYMBOL format controls
            0xE0001 |              // LANGUAGE TAG
            0xE0020..=0xE007F      // TAG characters
        )

        // Note: We don't check for PrivateUse (Co) or NotAssigned (Cn) categories
        // as these may legitimately appear in source files and should be preserved
    }

    if !s.chars().any(is_non_printable) {
        return std::borrow::Cow::Borrowed(s);
    }

    let mut result = String::with_capacity(s.len() + 10);
    for ch in s.chars() {
        if is_non_printable(ch) {
            // Non-printable character - escape as \xH (minimal hex, no leading zeros)
            // Matches Haskell's showHex behavior
            result.push_str(&format!("\\x{:x}", ch as u32));
        } else {
            result.push(ch);
        }
    }
    std::borrow::Cow::Owned(result)
}

/// sub_expr from pretty-simple's subExpr - formats a single expression with appropriate spacing
/// From Haskell:
///   subExpr x = let doc = prettyExpr opts x
///               in if isSimple x
///                  then nest 2 doc  -- space before simple
///                  else nest indentAmount $ line' <> doc  -- newline before complex
pub(crate) fn sub_expr<T: PrettySimple, W: Writer>(w: &mut W, arg: &T) {
    if arg.is_simple() {
        w.write_plain(" ");
        arg.format(w);
    } else if arg.has_delimiters() {
        w.newline();
        arg.format(w);
    } else {
        // Complex argument: wrap in parens. `renders_inline_parens` only controls
        // whether the closing paren stays on the same line or drops to the next.
        let inline = arg.renders_inline_parens();
        w.newline();
        with_brackets(w, "(", ")", true, |w, _| {
            w.write_plain(" ");
            arg.format(w);
            if inline {
                w.write_plain(" ")
            } else {
                w.newline()
            }
        });
    }
}

/// Common scaffold for delimiter-wrapped output.
///
/// Performs the exact `with_color → current_color → (optional with_depth) →
/// open … body … close` sequence that previously appeared open-coded at every
/// bracket / brace / paren site. `body` receives the writer and the captured
/// delimiter color so it can emit matching commas.
pub(crate) fn with_brackets<W: Writer>(
    w: &mut W,
    open: &str,
    close: &str,
    bump_depth: bool,
    body: impl FnOnce(&mut W, &'static str),
) {
    w.with_color(|w| {
        let delim_color = w.current_color();
        let inner = |w: &mut W| {
            w.write_colored(open, delim_color);
            body(w, delim_color);
            w.write_colored(close, delim_color);
        };
        if bump_depth {
            w.with_depth(inner);
        } else {
            inner(w);
        }
    });
}

/// Helper for formatting delimited values in lists and records
/// Handles spacing for simple vs delimited entries
///
/// Logic (unified from list_elem and format_record_value):
/// - Non-empty, complex delimited values get a newline before them
/// - Simple delimited values (like [ EmptyLine ]) stay inline
/// - Everything else: space before
pub(crate) fn format_delimited_value<T: PrettySimple, W: Writer>(w: &mut W, value: &T) {
    if value.has_delimiters() && !value.is_empty() && !value.is_simple() {
        w.newline();
        value.format(w);
    } else {
        w.write_plain(" ");
        value.format(w);
    }
}

/// Shared bracket-list rendering for `Vec<T>` and `IR`.
///
/// Mirrors pretty-simple's `list "[" "]"` logic:
/// - empty            → `[]`
/// - one simple row   → `[ e1 e2 ... ]` (inline)
/// - otherwise        → multiline, comma-first
///
/// `bump_depth` controls whether the body is rendered at one extra indentation
/// level. `Vec<T>` does this (matching pretty-simple's `Open` annotation),
/// whereas the top-level `IR` dump stays at depth 0 so its first column lines
/// up with the Haskell reference output.
pub(crate) fn format_bracket_list<T: PrettySimple, W: Writer>(
    w: &mut W,
    items: &[T],
    bump_depth: bool,
) {
    if items.is_empty() {
        with_brackets(w, "[", "]", false, |_, _| {});
        return;
    }

    with_brackets(w, "[", "]", bump_depth, |w, bracket_color| {
        if items.len() == 1 && items[0].is_simple() {
            w.write_plain(" ");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    w.write_plain(" ");
                }
                item.format(w);
            }
            w.write_plain(" ");
        } else {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    w.newline();
                    w.write_colored(",", bracket_color);
                }
                format_delimited_value(w, item);
            }
            w.newline();
        }
    });
}

/// Macro to format constructor applications
/// Based on pretty-simple's: Parens (CommaSeparated [[Other "Constructor", arg1, arg2, ...]])
/// Uses subExpr logic: simple elements get space before, complex get newline
/// Usage: format_constructor!(w, "ConstructorName", [arg1, arg2, arg3])
#[macro_export]
macro_rules! format_constructor {
    // Constructor with no arguments
    ($w:expr, $name:expr, []) => {
        $w.write_plain($name);
    };

    // Constructor with arguments - uses sub_expr for each
    ($w:expr, $name:expr, [ $($arg:expr),+ $(,)? ]) => {{
        $w.write_plain($name);
        $(
            $crate::pretty_simple::sub_expr($w, $arg);
        )*
    }};
}

/// Macro to format record fields with comma separation
/// Based on pretty-simple's list function for Braces
/// From Haskell: Braces xss -> list "{" "}" xss
///
/// Usage: format_record!(w, [("field1", &value1), ("field2", &value2), ...])
#[macro_export]
macro_rules! format_record {
    ($w:expr, [ $(($name:expr, $value:expr)),+ $(,)? ]) => {{
        $w.newline();
        $crate::pretty_simple::with_brackets($w, "{", "}", true, |w, brace_color| {
            format_record!(@fields w, brace_color; ; $( ($name, $value) ),+);
            w.newline();
        });
    }};

    // Base case: first field
    (@fields $w:expr, $brace_color:expr; ; ($name:expr, $value:expr) $(, ($rest_name:expr, $rest_value:expr))* ) => {
        $w.write_plain(" ");
        $w.write_plain($name);
        $w.write_plain(" =");
        $crate::pretty_simple::format_delimited_value($w, $value);
        $(
            format_record!(@fields $w, $brace_color; comma; ($rest_name, $rest_value));
        )*
    };

    // Recursive case: subsequent fields (with comma)
    (@fields $w:expr, $brace_color:expr; comma; ($name:expr, $value:expr)) => {
        $w.newline();
        $w.write_colored(",", $brace_color);
        $w.write_plain(" ");
        $w.write_plain($name);
        $w.write_plain(" =");
        $crate::pretty_simple::format_delimited_value($w, $value);
    };
}

/// Macro to format enum match arms
/// Automatically generates match arms that call format_constructor! for each variant
///
/// Usage without wildcard:
/// ```ignore
/// format_enum!(self, w, {
///     Variant1(field) => [field],
///     Variant2(field1, field2) => [field1, field2],
/// });
/// ```
///
/// Usage with wildcard (for fallback case):
/// ```ignore
/// format_enum!(self, w, {
///     Variant1(field) => [field],
///     _ => { w.write_plain(&format!("{:?}", self)); }
/// });
/// ```
#[macro_export]
macro_rules! format_enum {
    // Version without wildcard
    ($self:expr, $w:expr, {
        $( $variant:ident $( ( $($field:ident),* $(,)? ) )? => [ $($arg:expr),* $(,)? ] ),* $(,)?
    }) => {
        match $self {
            $(
                Self::$variant $( ( $($field),* ) )? => {
                    format_constructor!($w, stringify!($variant), [$($arg),*]);
                }
            )*
        }
    };

    // Version with wildcard
    ($self:expr, $w:expr, {
        $( $variant:ident $( ( $($field:ident),* $(,)? ) )? => [ $($arg:expr),* $(,)? ] ),* ,
        _ => $wildcard_body:block $(,)?
    }) => {
        match $self {
            $(
                Self::$variant $( ( $($field),* ) )? => {
                    format_constructor!($w, stringify!($variant), [$($arg),*]);
                }
            )*
            _ => $wildcard_body
        }
    };
}
