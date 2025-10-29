//! nixfmt-rs2 CLI

use nixfmt_rs::colored_writer::ColoredWriter;
use nixfmt_rs::error::context::ErrorContext;
use nixfmt_rs::error::format::ErrorFormatter;
use nixfmt_rs::pretty_simple::PrettySimple;
use nixfmt_rs::ParseError;
use std::io::{self, Read};
use std::process::exit;

fn handle_error(source: &str, filename: Option<&str>, e: ParseError) -> ! {
    let context = ErrorContext::new(source, filename);
    let formatter = ErrorFormatter::new(&context);
    eprintln!("{}", formatter.format(&e));
    exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut dump_ast = false;
    let mut dump_ir = false;
    let mut files = Vec::new();

    // Parse arguments
    for arg in &args[1..] {
        if arg == "--ast" {
            dump_ast = true;
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
    if dump_ast {
        let file = nixfmt_rs::parse(&source).unwrap_or_else(|e| handle_error(&source, filename, e));
        let mut writer = ColoredWriter::new(&source);
        file.format(&mut writer);
        print!("{}", writer.finish());
    } else if dump_ir {
        use nixfmt_rs::predoc::{fixup, Pretty};
        let file = nixfmt_rs::parse(&source).unwrap_or_else(|e| handle_error(&source, filename, e));
        let doc = file.pretty();
        let doc = fixup(&doc);
        let mut writer = ColoredWriter::new(&source);
        doc.format(&mut writer);
        print!("{}", writer.finish());
    } else {
        let formatted =
            nixfmt_rs::format(&source).unwrap_or_else(|e| handle_error(&source, filename, e));
        print!("{}", formatted);
    }
}
