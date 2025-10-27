//! Example demonstrating error visualization
//!
//! This example shows what error messages look like for common parsing errors.
//! Run with: cargo run --example error_visualization

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
            code: r#"{
  services.nginx.enable = true
  networking.firewall.enable = false;
}"#,
        },
        ErrorExample {
            name: "unclosed_brace",
            description: "Unclosed brace in attribute set",
            code: r#"{
  foo = 1;
  bar = {
    baz = 2;
  # missing closing brace
}"#,
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
            code: r#"1 < 2 < 3"#,
        },
        ErrorExample {
            name: "unexpected_token",
            description: "Unexpected token (using 'in' without 'let')",
            code: r#"{
  foo = 1;
  in bar
}"#,
        },
        ErrorExample {
            name: "invalid_lambda_pattern",
            description: "Invalid syntax in lambda parameter",
            code: r#"x @ y @ z: x + y + z"#,
        },
        ErrorExample {
            name: "missing_then",
            description: "Missing 'then' in if expression",
            code: r#"if true else false"#,
        },
        ErrorExample {
            name: "unclosed_parenthesis",
            description: "Unclosed parenthesis",
            code: r#"(1 + 2"#,
        },
        ErrorExample {
            name: "mismatched_braces",
            description: "Mismatched delimiters",
            code: r#"{
  foo = [1, 2, 3};
}"#,
        },
        ErrorExample {
            name: "trailing_slash_in_path",
            description: "Path cannot end with trailing slash",
            code: r#"./path/to/directory/"#,
        },
        ErrorExample {
            name: "double_negation_spacing",
            description: "Ambiguous negation (parsed correctly but interesting)",
            code: r#"- -5"#,
        },
        ErrorExample {
            name: "unclosed_indented_string",
            description: "Unclosed indented string",
            code: r#"''
  This is an indented string
  that is never closed
"#,
        },
        ErrorExample {
            name: "missing_colon_in_lambda",
            description: "Missing colon after lambda parameter",
            code: r#"x x + 1"#,
        },
        ErrorExample {
            name: "invalid_operator",
            description: "Invalid operator usage",
            code: r#"let x = 1 & 2; in x"#,
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
                for line in error.to_string().lines() {
                    println!("│ {}", line);
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
            println!("│ Error: Missing semicolon after attribute definition");
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
            println!("│ Error: Unclosed delimiter");
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
            println!("│ Error: Unclosed string literal");
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
            println!("│ Error: Comparison operators cannot be chained");
            println!("│   ┌─ <input>:1:3");
            println!("│   │");
            println!("│ 1 │ 1 < 2 < 3");
            println!("│   │   ^   ^ cannot chain comparison operators");
            println!("│   │");
            println!("│   = note: use parentheses to clarify: (1 < 2) && (2 < 3)");
        }
        "unexpected_token" => {
            println!("│ Error: Unexpected token 'in'");
            println!("│   ┌─ <input>:3:3");
            println!("│   │");
            println!("│ 3 │   in bar");
            println!("│   │   ^^ unexpected keyword");
            println!("│   │");
            println!("│   = note: 'in' can only be used with 'let ... in ...' expressions");
            println!("│   = help: did you mean to use 'let' before the bindings?");
        }
        "invalid_lambda_pattern" => {
            println!("│ Error: Invalid lambda parameter");
            println!("│   ┌─ <input>:1:6");
            println!("│   │");
            println!("│ 1 │ x @ y @ z: x + y + z");
            println!("│   │       ^^^ cannot chain @ in parameters");
            println!("│   │");
            println!("│   = note: @ can only be used once in a parameter pattern");
            println!("│   = help: use: x @ {{ y, z }}: ... to destructure with name binding");
        }
        "missing_then" => {
            println!("│ Error: Expected 'then' after condition");
            println!("│   ┌─ <input>:1:9");
            println!("│   │");
            println!("│ 1 │ if true else false");
            println!("│   │         ^^^^ expected 'then', found 'else'");
            println!("│   │");
            println!("│   = note: if expressions require: if <condition> then <expr> else <expr>");
            println!("│   = help: add 'then' before the true branch");
        }
        "unclosed_parenthesis" => {
            println!("│ Error: Unclosed delimiter");
            println!("│   ┌─ <input>:1:1");
            println!("│   │");
            println!("│ 1 │ (1 + 2");
            println!("│   │ ^ opened here");
            println!("│   │       ^ expected ')', found EOF");
            println!("│   │");
            println!("│   = help: add closing parenthesis: (1 + 2)");
        }
        "mismatched_braces" => {
            println!("│ Error: Mismatched delimiter");
            println!("│   ┌─ <input>:2:19");
            println!("│   │");
            println!("│ 2 │   foo = [1, 2, 3}};");
            println!("│   │         ^       ^ expected ']', found '}}'");
            println!("│   │         │");
            println!("│   │         list opened with '[' here");
            println!("│   │");
            println!("│   = help: change '}}' to ']' to match the opening bracket");
        }
        "trailing_slash_in_path" => {
            println!("│ Error: Path cannot end with trailing slash");
            println!("│   ┌─ <input>:1:23");
            println!("│   │");
            println!("│ 1 │ ./path/to/directory/");
            println!("│   │ ^^^^^^^^^^^^^^^^^^^^ path ends with '/'");
            println!("│   │");
            println!("│   = note: Nix paths must have content after the final '/'");
            println!("│   = help: remove the trailing slash: ./path/to/directory");
        }
        "double_negation_spacing" => {
            println!("│ Note: This parses correctly as negation of negation");
            println!("│   ┌─ <input>:1:1");
            println!("│   │");
            println!("│ 1 │ - -5");
            println!("│   │ ^^^^ evaluates to 5");
            println!("│   │");
            println!("│   = note: parsed as: -(-(5))");
        }
        "unclosed_indented_string" => {
            println!("│ Error: Unclosed indented string");
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
        "missing_colon_in_lambda" => {
            println!("│ Error: Missing ':' in lambda expression");
            println!("│   ┌─ <input>:1:3");
            println!("│   │");
            println!("│ 1 │ x x + 1");
            println!("│   │   ^ expected ':', found identifier");
            println!("│   │");
            println!("│   = note: lambda syntax is: <param>: <body>");
            println!("│   = help: add colon after parameter: x: x + 1");
        }
        "invalid_operator" => {
            println!("│ Error: Unexpected '&'");
            println!("│   ┌─ <input>:1:13");
            println!("│   │");
            println!("│ 1 │ let x = 1 & 2; in x");
            println!("│   │           ^ unexpected '&'");
            println!("│   │");
            println!("│   = note: single '&' is not an operator in Nix");
            println!("│   = help: did you mean '&&' (logical AND)?");
        }
        _ => {
            println!("│ (Future enhanced error message for: {})", example.name);
        }
    }
}
