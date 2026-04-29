//! nixfmt-rs2 CLI

use std::io::{self, Read};
use std::process::exit;

fn handle_error(source: &str, filename: Option<&str>, e: nixfmt_rs::ParseError) -> ! {
    eprintln!("{}", nixfmt_rs::format_error(source, filename, &e));
    exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut dump_ast = false;
    let mut dump_ir = false;
    let mut parse_only = false;
    let mut files = Vec::new();

    // Parse arguments
    for arg in &args[1..] {
        if arg == "--ast" {
            dump_ast = true;
        } else if arg == "--parse-only" {
            parse_only = true;
        } else if arg == "--ir" {
            dump_ir = true;
        } else if !arg.starts_with('-') {
            files.push(arg.clone());
        }
    }

    // Read from stdin if no files specified
    let (source, filename) = if files.is_empty() {
        let mut buffer = String::new();
        match io::stdin().read_to_string(&mut buffer) {
            Ok(_) => (buffer, None),
            Err(e) => {
                eprintln!("error: failed to read stdin: {}", e);
                exit(1);
            }
        }
    } else {
        match std::fs::read_to_string(&files[0]) {
            Ok(content) => (content, Some(files[0].as_str())),
            Err(e) => {
                eprintln!("error: failed to read file '{}': {}", files[0], e);
                exit(1);
            }
        }
    };

    // Process based on mode
    if parse_only {
        nixfmt_rs::parse(&source).unwrap_or_else(|e| handle_error(&source, filename, e));
    } else if dump_ast {
        let output =
            nixfmt_rs::format_ast(&source).unwrap_or_else(|e| handle_error(&source, filename, e));
        print!("{}", output);
    } else if dump_ir {
        let output =
            nixfmt_rs::format_ir(&source).unwrap_or_else(|e| handle_error(&source, filename, e));
        print!("{}", output);
    } else {
        let output =
            nixfmt_rs::format(&source).unwrap_or_else(|e| handle_error(&source, filename, e));
        print!("{}", output);
    }
}
