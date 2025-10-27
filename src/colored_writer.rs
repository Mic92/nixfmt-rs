//! Colored Writer implementation using the trait-based approach

use crate::pretty_simple::Writer;

// ANSI color codes
const RESET: &str = "\x1b[0m";

const DELIM_COLORS: &[&str] = &[
    "\x1b[0;95;1m", // colorBold Vivid Magenta
    "\x1b[0;96;1m", // colorBold Vivid Cyan
    "\x1b[0;93;1m", // colorBold Vivid Yellow
    "\x1b[0;35m",   // color Dull Magenta
    "\x1b[0;36m",   // color Dull Cyan
    "\x1b[0;33m",   // color Dull Yellow
    "\x1b[0;35;1m", // colorBold Dull Magenta
    "\x1b[0;36;1m", // colorBold Dull Cyan
    "\x1b[0;33;1m", // colorBold Dull Yellow
    "\x1b[0;95m",   // color Vivid Magenta
    "\x1b[0;96m",   // color Vivid Cyan
    "\x1b[0;93m",   // color Vivid Yellow
];

const ERROR_COLOR: &str = "\x1b[0;91;1m";

pub struct ColoredWriter<'a> {
    depth: usize,
    color_depth: usize,
    output: String,
    line_start: bool,
    source: &'a str,
}

impl<'a> ColoredWriter<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            depth: 0,
            color_depth: 0,
            output: String::new(),
            line_start: true,
            source,
        }
    }

    pub fn finish(self) -> String {
        self.output
    }

    fn indent(&mut self) {
        if self.line_start {
            self.output.push_str(&" ".repeat(self.depth * 4));
            self.line_start = false;
        }
    }
}

impl<'a> Writer for ColoredWriter<'a> {
    fn write_plain(&mut self, text: &str) {
        self.indent();
        self.output.push_str(text);
    }

    fn write_colored(&mut self, text: &str, color: &str) {
        self.indent();
        self.output.push_str(color);
        self.output.push_str(text);
        self.output.push_str(RESET);
    }

    fn newline(&mut self) {
        self.output.push('\n');
        self.line_start = true;
    }

    fn with_color<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.color_depth += 1;
        let result = f(self);
        self.color_depth -= 1;
        result
    }

    fn current_color(&self) -> &'static str {
        if self.color_depth == 0 {
            ERROR_COLOR
        } else {
            let index = (self.color_depth - 1) % DELIM_COLORS.len();
            DELIM_COLORS[index]
        }
    }

    fn with_depth<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }

    fn source(&self) -> &str {
        self.source
    }
}
