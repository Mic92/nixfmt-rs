//! Example demonstrating error visualization
//!
//! This example shows what error messages look like for common parsing errors.
//! Run with: cargo run --example `error_visualization`

use nixfmt_rs::error::context::ErrorContext;
use nixfmt_rs::error::format::ErrorFormatter;
use nixfmt_rs::parse;

/// A test case with intentionally broken Nix code
struct ErrorExample {
    name: &'static str,
    description: &'static str,
    code: &'static str,
}

fn main() {
    let examples = vec![
        ErrorExample {
            name: "missing_semicolon",
            description: "Missing semicolon after attribute definition",
            code: r"{
  services.nginx.enable = true
  networking.firewall.enable = false;
}",
        },
        ErrorExample {
            name: "missing_semicolon_in_let",
            description: "Missing semicolon in let binding",
            code: r"let
  x = 1
  y = 2;
in x + y",
        },
        ErrorExample {
            name: "missing_semicolon_nested",
            description: "Missing semicolon in nested attribute set",
            code: r"{
  foo = {
    bar = 1
    baz = 2;
  };
}",
        },
        ErrorExample {
            name: "unclosed_brace",
            description: "Unclosed brace in attribute set",
            code: r"{
  foo = 1;
  bar = {
    baz = 2;
  # missing closing brace
}",
        },
        ErrorExample {
            name: "unclosed_string",
            description: "Unclosed string literal",
            code: r#"{
  message = "Hello, world!
  x = 42;
}"#,
        },
        ErrorExample {
            name: "chained_comparison",
            description: "Chained comparison operators (not allowed)",
            code: r"1 < 2 < 3",
        },
        ErrorExample {
            name: "unexpected_token",
            description: "Unexpected token (using 'in' without 'let')",
            code: r"{
  foo = 1;
  in bar
}",
        },
        ErrorExample {
            name: "missing_then",
            description: "Missing 'then' in if expression",
            code: r"if true else false",
        },
        ErrorExample {
            name: "unclosed_parenthesis",
            description: "Unclosed parenthesis",
            code: r"(1 + 2",
        },
        ErrorExample {
            name: "unclosed_bracket",
            description: "Unclosed list bracket",
            code: r"[1 2 3",
        },
        ErrorExample {
            name: "mismatched_braces",
            description: "Mismatched delimiters (list with wrong closing)",
            code: r"{
  foo = [1 2 3};
}",
        },
        ErrorExample {
            name: "mismatched_parenthesis",
            description: "Mismatched delimiters (parenthesis with bracket)",
            code: r"(1 + 2]",
        },
        ErrorExample {
            name: "comma_in_list",
            description: "Commas not allowed in Nix lists",
            code: r"[1, 2, 3]",
        },
        ErrorExample {
            name: "trailing_slash_in_path",
            description: "Path cannot end with trailing slash",
            code: r"./path/to/directory/",
        },
        ErrorExample {
            name: "unclosed_indented_string",
            description: "Unclosed indented string",
            code: r"''
  This is an indented string
  that is never closed
",
        },
        ErrorExample {
            name: "invalid_operator",
            description: "Invalid operator usage",
            code: r"let x = 1 & 2; in x",
        },
        ErrorExample {
            name: "empty_interpolation",
            description: "Empty string interpolation",
            code: r#""hello ${}""#,
        },
        ErrorExample {
            name: "incomplete_interpolation_expr",
            description: "Incomplete expression in interpolation",
            code: r#""result: ${1 + }""#,
        },
        ErrorExample {
            name: "unclosed_interpolation",
            description: "Unclosed string with interpolation",
            code: r#""hello ${name"#,
        },
        ErrorExample {
            name: "unclosed_interpolation_in_binding",
            description: "Unclosed string with interpolation in let binding (wrong position bug)",
            code: r#"let
  name = "World";
  greeting = "Hello ${name";
in greeting"#,
        },
        ErrorExample {
            name: "attribute_path_no_value",
            description: "Attribute path without assignment (forgot = and value)",
            code: r#"{
  networking.interfaces."eth0";
}"#,
        },
        ErrorExample {
            name: "function_call_with_commas",
            description: "Function call with commas (common mistake from other languages)",
            code: r"let
  add = x: y: x + y;
  result = add(1, 2);
in result",
        },
    ];

    println!("{}", "=".repeat(80));
    println!("NIXFMT ERROR VISUALIZATION");
    println!("{}", "=".repeat(80));
    println!();
    println!("This example shows current error output for common parsing errors.");
    println!("Future enhancements will make these much more helpful!");
    println!();

    for (i, example) in examples.iter().enumerate() {
        println!("\n{}", "─".repeat(80));
        println!("Example {}/{}: {}", i + 1, examples.len(), example.name);
        println!("{}", "─".repeat(80));
        println!("Description: {}", example.description);
        println!();
        println!("Code:");
        println!("{}", "┌".to_owned() + &"─".repeat(78));
        for (line_no, line) in example.code.lines().enumerate() {
            println!("│ {:2} │ {}", line_no + 1, line);
        }
        println!("{}", "└".to_owned() + &"─".repeat(78));
        println!();

        // Parse and show error
        println!("Current error output:");
        match parse(example.code) {
            Ok(_) => {
                println!("✓ Parsed successfully (no error)");
            }
            Err(error) => {
                println!("{}", "┌".to_owned() + &"─".repeat(78));

                // Use the new ErrorFormatter for beautiful output
                let context = ErrorContext::new(example.code, Some("<input>"));
                let formatter = ErrorFormatter::new(&context);
                let rendered = formatter.format(&error);

                for line in rendered.lines() {
                    println!("│ {line}");
                }
                println!("{}", "└".to_owned() + &"─".repeat(78));
            }
        }

        println!();
        println!("Future error output (goal):");
        println!("{}", "┌".to_owned() + &"─".repeat(78));
        print_future_error(example);
        println!("{}", "└".to_owned() + &"─".repeat(78));
    }

    println!("\n{}", "=".repeat(80));
    println!("END OF ERROR EXAMPLES");
    println!("{}", "=".repeat(80));
}

/// Show what the error *should* look like with enhanced formatting
fn print_future_error(example: &ErrorExample) {
    match example.name {
        "missing_semicolon" => {
            println!("│ Error[E001]: Missing semicolon after attribute definition");
            println!("│   ┌─ <input>:2:35");
            println!("│   │");
            println!("│ 2 │   services.nginx.enable = true");
            println!("│   │                                   ^ expected ';' here");
            println!("│ 3 │   networking.firewall.enable = false;");
            println!("│   │   ──────────────────────────────────── next attribute starts here");
            println!("│   │");
            println!("│   = note: attribute definitions in sets must be terminated with ';'");
            println!("│   = help: add a semicolon: `services.nginx.enable = true;`");
        }
        "unclosed_brace" => {
            println!("│ Error[E002]: Unclosed delimiter");
            println!("│   ┌─ <input>:1:1");
            println!("│   │");
            println!("│ 1 │ {{");
            println!("│   │ ^ this brace was opened here");
            println!("│   │");
            println!("│ 6 │   # missing closing brace");
            println!("│ 7 │ }}");
            println!("│   │ ^ expected '}}' but found EOF");
            println!("│   │");
            println!("│   = note: unmatched opening brace");
            println!("│   = help: add a closing brace '}}' at the end");
        }
        "unclosed_string" => {
            println!("│ Error[E002]: Unclosed string literal");
            println!("│   ┌─ <input>:2:13");
            println!("│   │");
            println!("│ 2 │   message = \"Hello, world!");
            println!("│   │             ^^^^^^^^^^^^^^^ string starts here");
            println!("│ 3 │   x = 42;");
            println!("│   │");
            println!("│   = note: string was never closed");
            println!("│   = help: add closing quote before the newline");
        }
        "chained_comparison" => {
            println!("│ Error[E006]: Comparison operators cannot be chained");
            println!("│   ┌─ <input>:1:3");
            println!("│   │");
            println!("│ 1 │ 1 < 2 < 3");
            println!("│   │   ^   ^ cannot chain comparison operators");
            println!("│   │");
            println!("│   = note: use parentheses to clarify: (1 < 2) && (2 < 3)");
        }
        "unexpected_token" => {
            println!("│ Error[E001]: Unexpected token 'in'");
            println!("│   ┌─ <input>:3:3");
            println!("│   │");
            println!("│ 3 │   in bar");
            println!("│   │   ^^ unexpected keyword");
            println!("│   │");
            println!("│   = note: 'in' can only be used with 'let ... in ...' expressions");
            println!("│   = help: did you mean to use 'let' before the bindings?");
        }
        "missing_then" => {
            println!("│ Error[E001]: Expected 'then' after condition");
            println!("│   ┌─ <input>:1:9");
            println!("│   │");
            println!("│ 1 │ if true else false");
            println!("│   │         ^^^^ expected 'then', found 'else'");
            println!("│   │");
            println!("│   = note: if expressions require: if <condition> then <expr> else <expr>");
            println!("│   = help: add 'then' before the true branch");
        }
        "unclosed_parenthesis" => {
            println!("│ Error[E002]: Unclosed delimiter");
            println!("│   ┌─ <input>:1:1");
            println!("│   │");
            println!("│ 1 │ (1 + 2");
            println!("│   │ ^ opened here");
            println!("│   │       ^ expected ')', found EOF");
            println!("│   │");
            println!("│   = help: add closing parenthesis: (1 + 2)");
        }
        "unclosed_bracket" => {
            println!("│ Error[E002]: Unclosed delimiter");
            println!("│   ┌─ <input>:1:1");
            println!("│   │");
            println!("│ 1 │ [1 2 3");
            println!("│   │ ^ opened here");
            println!("│   │       ^ expected ']', found EOF");
            println!("│   │");
            println!("│   = help: add closing bracket: [1 2 3]");
        }
        "mismatched_braces" => {
            println!("│ Error[E005]: Mismatched delimiter");
            println!("│   ┌─ <input>:2:9");
            println!("│   │");
            println!("│ 2 │   foo = [1 2 3}};");
            println!("│   │         ^     ^ expected ']', found '}}'");
            println!("│   │         │");
            println!("│   │         list opened with '[' here");
            println!("│   │");
            println!("│   = help: change '}}' to ']' to match the opening bracket");
        }
        "mismatched_parenthesis" => {
            println!("│ Error[E001]: Mismatched delimiter");
            println!("│   ┌─ <input>:1:1");
            println!("│   │");
            println!("│ 1 │ (1 + 2]");
            println!("│   │ ^     ^ expected ')', found ']'");
            println!("│   │");
            println!("│   = help: change ']' to ')' to match the opening parenthesis");
        }
        "comma_in_list" => {
            println!("│ Error[E005]: Commas not allowed in Nix lists");
            println!("│   ┌─ <input>:1:3");
            println!("│   │");
            println!("│ 1 │ [1, 2, 3]");
            println!("│   │   ^ commas are not used to separate list elements");
            println!("│   │");
            println!("│   = note: Nix uses whitespace to separate list elements");
            println!("│   = help: use spaces instead: [1 2 3]");
        }
        "trailing_slash_in_path" => {
            println!("│ Error[E005]: Path cannot end with trailing slash");
            println!("│   ┌─ <input>:1:23");
            println!("│   │");
            println!("│ 1 │ ./path/to/directory/");
            println!("│   │ ^^^^^^^^^^^^^^^^^^^^ path ends with '/'");
            println!("│   │");
            println!("│   = note: Nix paths must have content after the final '/'");
            println!("│   = help: remove the trailing slash: ./path/to/directory");
        }
        "unclosed_indented_string" => {
            println!("│ Error[E002]: Unclosed indented string");
            println!("│   ┌─ <input>:1:1");
            println!("│   │");
            println!("│ 1 │ ''");
            println!("│   │ ^^ string starts here");
            println!("│ 2 │   This is an indented string");
            println!("│ 3 │   that is never closed");
            println!("│   │");
            println!("│   = note: indented strings must end with ''");
            println!("│   = help: add '' at the end of the string");
        }
        "invalid_operator" => {
            println!("│ Error[E001]: Unexpected '&'");
            println!("│   ┌─ <input>:1:13");
            println!("│   │");
            println!("│ 1 │ let x = 1 & 2; in x");
            println!("│   │           ^ unexpected '&'");
            println!("│   │");
            println!("│   = note: single '&' is not an operator in Nix");
            println!("│   = help: did you mean '&&' (logical AND)?");
        }
        "missing_semicolon_in_let" => {
            println!("│ Error[E001]: Missing semicolon after let binding");
            println!("│   ┌─ <input>:2:8");
            println!("│   │");
            println!("│ 2 │   x = 1");
            println!("│   │        ^ expected ';' here");
            println!("│ 3 │   y = 2;");
            println!("│   │   ────── next binding starts here");
            println!("│   │");
            println!("│   = note: bindings in let expressions must be terminated with ';'");
            println!("│   = help: add a semicolon: `x = 1;`");
        }
        "missing_semicolon_nested" => {
            println!("│ Error[E001]: Missing semicolon after attribute definition");
            println!("│   ┌─ <input>:3:14");
            println!("│   │");
            println!("│ 3 │     bar = 1");
            println!("│   │            ^ expected ';' here");
            println!("│ 4 │     baz = 2;");
            println!("│   │     ────── next attribute starts here");
            println!("│   │");
            println!("│   = note: attribute definitions in sets must be terminated with ';'");
            println!("│   = help: add a semicolon: `bar = 1;`");
        }
        "empty_interpolation" => {
            println!("│ Error[E001]: Empty interpolation expression");
            println!("│   ┌─ <input>:1:9");
            println!("│   │");
            println!("│ 1 │ \\\"hello ${{}}\\\"");
            println!("│   │          ^ expected expression, found '}}'");
            println!("│   │");
            println!("│   = note: string interpolations require an expression inside ${{...}}");
            println!(
                "│   = help: add an expression: ${{variableName}} or remove the empty interpolation"
            );
        }
        "incomplete_interpolation_expr" => {
            println!("│ Error[E001]: Incomplete expression in interpolation");
            println!("│   ┌─ <input>:1:17");
            println!("│   │");
            println!("│ 1 │ \\\"result: ${{1 + }}\\\"");
            println!("│   │                  ^ expected expression after operator");
            println!("│   │");
            println!("│   = note: binary operators require expressions on both sides");
            println!("│   = help: complete the expression: ${{1 + 2}}");
        }
        "unclosed_interpolation" => {
            println!("│ Error[E002]: Unclosed string literal with interpolation");
            println!("│   ┌─ <input>:1:1");
            println!("│   │");
            println!("│ 1 │ \\\"hello ${{name\\\"");
            println!("│   │ ^ string starts here");
            println!("│   │        ^^^^^^^ interpolation starts here");
            println!("│   │");
            println!("│   = note: string was never closed (missing closing quote)");
            println!(
                "│   = help: add }} to close interpolation and \\\" to close string: \\\"hello ${{name}}\\\""
            );
        }
        "unclosed_interpolation_in_binding" => {
            println!("│ Error[E002]: Unclosed string literal with interpolation");
            println!("│   ┌─ <input>:3:13");
            println!("│   │");
            println!("│ 1 │ let");
            println!("│ 2 │   name = \\\"World\\\";");
            println!("│ 3 │   greeting = \\\"Hello ${{name\\\";");
            println!(
                "│   │              ^ string starts here (currently WRONG: reports column 26)"
            );
            println!("│   │                     ^^^^^^^ interpolation starts here");
            println!("│ 4 │ in greeting");
            println!("│   │");
            println!("│   = note: string was never closed (missing closing quote)");
            println!("│   = help: add }} to close interpolation and \\\" to close string");
        }
        "attribute_path_no_value" => {
            println!("│ Error[E001]: Expected '=' after attribute path");
            println!("│   ┌─ <input>:2:30");
            println!("│   │");
            println!("│ 2 │   networking.interfaces.\\\"eth0\\\";");
            println!("│   │                               ^ expected '=' to assign a value");
            println!("│   │");
            println!("│   = note: attribute paths must be followed by '= <value>;'");
            println!(
                "│   = help: add an assignment: networking.interfaces.\\\"eth0\\\" = {{ ... }};"
            );
        }
        "function_call_with_commas" => {
            println!("│ Error[E001]: Comma not allowed inside parentheses");
            println!("│   ┌─ <input>:3:16");
            println!("│   │");
            println!("│ 3 │   result = add(1, 2);");
            println!("│   │                 ^ unexpected comma");
            println!("│   │");
            println!("│   = note: Nix doesn't use commas in parenthesized expressions");
            println!("│   = help: for function calls, use spaces: add 1 2");
            println!(
                "│   = help: for multiple values, use a list [1 2] or set {{ x = 1; y = 2; }}"
            );
        }
        _ => {
            println!("│ (Future enhanced error message for: {})", example.name);
        }
    }
}
