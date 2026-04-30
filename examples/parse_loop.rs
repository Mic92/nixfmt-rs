//! Tight parse loop for profiling: reads a file once, parses N times.
use std::fs;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: parse_loop <file> [iters]");
    let iters: usize = args
        .next()
        .map_or(100, |s| s.parse().expect("iters must be a number"));
    let src = fs::read_to_string(&path).expect("read");
    let t0 = std::time::Instant::now();
    for _ in 0..iters {
        let f = nixfmt_rs::parse(&src).expect("parse");
        std::hint::black_box(f);
    }
    let dt = t0.elapsed();
    eprintln!(
        "{} iters in {:?} ({:.2} ms/iter)",
        iters,
        dt,
        dt.as_secs_f64() * 1000.0 / iters as f64
    );
}
