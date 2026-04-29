use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn get_nixpkgs_path() -> PathBuf {
    // Get nixpkgs source path from flake inputs
    let output = Command::new("nix")
        .args([
            "eval",
            "--raw",
            "--impure",
            "--expr",
            &format!(
                "(builtins.getFlake \"git+file://{}\").inputs.nixpkgs.outPath",
                std::env::current_dir().unwrap().display()
            ),
        ])
        .output()
        .expect("Failed to get nixpkgs path");

    PathBuf::from(String::from_utf8_lossy(&output.stdout).to_string())
}

fn benchmark_parser(c: &mut Criterion) {
    // Get nixpkgs path from flake
    let nixpkgs_path = get_nixpkgs_path();

    // Paths to nixpkgs files for benchmarking
    let small_file = nixpkgs_path.join("nixos/modules/services/networking/ssh/sshd.nix");
    let large_file = nixpkgs_path.join("pkgs/top-level/all-packages.nix");
    let very_large_file = nixpkgs_path.join("maintainers/maintainer-list.nix");

    // Read file contents once
    let small_content = fs::read_to_string(small_file).expect("Failed to read small file");
    let large_content = fs::read_to_string(large_file).expect("Failed to read large file");
    let very_large_content =
        fs::read_to_string(very_large_file).expect("Failed to read very large file");

    c.bench_function("parser_small_file", |b| {
        b.iter(|| nixfmt_rs::parse(black_box(&small_content)).unwrap())
    });

    c.bench_function("parser_large_file", |b| {
        b.iter(|| nixfmt_rs::parse(black_box(&large_content)).unwrap())
    });

    c.bench_function("parser_very_large_file", |b| {
        b.iter(|| nixfmt_rs::parse(black_box(&very_large_content)).unwrap())
    });
}

criterion_group!(benches, benchmark_parser);
criterion_main!(benches);
