//! nixfmt-rs2 CLI

use nixfmt_rs::colored_writer::ColoredWriter;
use nixfmt_rs::error::context::ErrorContext;
use nixfmt_rs::error::format::ErrorFormatter;
use nixfmt_rs::pretty_simple::PrettySimple;
use std::io::{self, Read};
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut dump_ast = false;
    let mut files = Vec::new();

    // Parse arguments
    for arg in &args[1..] {
        if arg == "--ast" {
            dump_ast = true;
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

    // Parse the file
    match nixfmt_rs::parse(&source) {
        Ok(file) => {
            if dump_ast {
                // Output AST with colors matching nixfmt
                let mut writer = ColoredWriter::new(&source);
                file.format(&mut writer);
                print!("{}", writer.finish());
            } else {
                eprintln!("nixfmt_rs: formatting not yet implemented");
                eprintln!("Use --ast to dump AST");
                exit(1);
            }
        }
        Err(e) => {
            // Use the new beautiful error formatter
            let context = ErrorContext::new(&source, filename);
            let formatter = ErrorFormatter::new(&context);
            eprintln!("{}", formatter.format(&e));
            exit(1);
        }
    }
}
