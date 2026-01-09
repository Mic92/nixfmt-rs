#!/usr/bin/env python3
"""Compare nixfmt_rs against nixfmt across the ../nixpkgs tree.

This script walks the nixpkgs checkout located next to this repository,
formats every *.nix file via both nixfmt_rs and the reference nixfmt,
and reports any mismatches. The work is parallelised to speed up runs.

Usage:
    ./test_nixpkgs_compare.py           # Compare --ast output (default)
    ./test_nixpkgs_compare.py --ir      # Compare --ir output
    ./test_nixpkgs_compare.py --format  # Compare formatted output
"""

import os
import sys
import subprocess
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import Tuple


NIXFMT_RS = "./target/debug/nixfmt_rs"
NIXFMT = "nixfmt"
REPO_ROOT = Path(__file__).parent.resolve()
NIXPKGS_ROOT = (REPO_ROOT / "../nixpkgs").resolve()
MAX_TIMEOUT = 120  # seconds per formatter invocation

# Global mode flag, set by command line args
COMPARE_MODE = "ast"  # "ast", "ir", or "format"


def detect_nixfmt_failure(stderr: str, returncode: int, path: Path) -> bool:
    """Heuristically determine whether nixfmt considered the file invalid."""
    if str(path) in stderr:
        return True
    prefix = stderr[:150].lower()
    if "error:" in prefix or "unexpected" in prefix:
        return True
    # nixfmt --ast returns 1 even on success, but other codes are real failures.
    return returncode not in (0, 1)


def compare_file(path: Path) -> Tuple[bool, str]:
    """Return (success, message) for a single file comparison."""
    try:
        expr = path.read_bytes()
    except Exception as exc:
        return False, f"failed to read file: {exc}"

    # Build command args based on mode
    if COMPARE_MODE == "format":
        nixfmt_args = [NIXFMT, str(path)]
        ours_args = [NIXFMT_RS]
    else:
        # --ast or --ir
        flag = f"--{COMPARE_MODE}"
        nixfmt_args = [NIXFMT, flag, str(path)]
        ours_args = [NIXFMT_RS, flag]

    try:
        nixfmt_proc = subprocess.run(
            nixfmt_args,
            capture_output=True,
            timeout=MAX_TIMEOUT,
            check=False,
        )
    except FileNotFoundError:
        return False, "nixfmt executable not found in PATH"
    except subprocess.TimeoutExpired:
        return False, "nixfmt timed out"

    nixfmt_stderr = nixfmt_proc.stderr.decode("utf-8", errors="replace").rstrip()
    nixfmt_failed = detect_nixfmt_failure(nixfmt_stderr, nixfmt_proc.returncode, path)

    try:
        ours_proc = subprocess.run(
            ours_args,
            input=expr,
            capture_output=True,
            timeout=MAX_TIMEOUT,
            check=False,
        )
    except FileNotFoundError:
        return False, "build artefact nixfmt_rs not found – run `cargo build` first"
    except subprocess.TimeoutExpired:
        return False, "nixfmt_rs timed out"

    our_output = ours_proc.stdout.decode("utf-8", errors="replace").rstrip()
    our_failed = ours_proc.returncode != 0

    if nixfmt_failed and not our_failed:
        return False, f"nixfmt rejects but nixfmt_rs accepts\n{nixfmt_stderr[:200]}"

    if not nixfmt_failed and our_failed:
        ours_stderr = ours_proc.stderr.decode("utf-8", errors="replace").rstrip()
        return False, f"nixfmt_rs rejects but nixfmt accepts\n{ours_stderr[:200]}"

    if nixfmt_failed and our_failed:
        return True, ""

    # For --ast and --ir, nixfmt outputs to stderr
    # For format mode, both output to stdout
    if COMPARE_MODE == "format":
        expected = nixfmt_proc.stdout.decode("utf-8", errors="replace").rstrip()
    else:
        expected = nixfmt_stderr

    if expected != our_output:
        mode_name = COMPARE_MODE.upper()
        return False, f"{mode_name} mismatch between nixfmt and nixfmt_rs"

    return True, ""


def gather_files(root: Path) -> list[Path]:
    if not root.exists():
        raise FileNotFoundError(f"nixpkgs checkout not found at {root}")
    return [p for p in root.rglob("*.nix") if p.is_file()]


def main() -> None:
    global COMPARE_MODE

    # Parse command line arguments
    if len(sys.argv) > 1:
        arg = sys.argv[1]
        if arg == "--ir":
            COMPARE_MODE = "ir"
        elif arg == "--format":
            COMPARE_MODE = "format"
        elif arg == "--ast":
            COMPARE_MODE = "ast"
        elif arg in ("--help", "-h"):
            print(__doc__)
            sys.exit(0)
        else:
            print(f"Unknown option: {arg}")
            print(__doc__)
            sys.exit(1)

    mode_desc = {
        "ast": "AST representations",
        "ir": "IR (Doc) representations",
        "format": "formatted output",
    }

    print(f"Mode: Comparing {mode_desc[COMPARE_MODE]}")
    print("Building nixfmt_rs...")
    subprocess.run(["cargo", "build", "--quiet"], check=True)

    try:
        nix_files = gather_files(NIXPKGS_ROOT)
    except FileNotFoundError as exc:
        print(f"❌ {exc}")
        sys.exit(1)

    total = len(nix_files)
    if total == 0:
        print(f"No .nix files found under {NIXPKGS_ROOT}")
        sys.exit(0)

    print(f"Found {total} .nix files under {NIXPKGS_ROOT}")
    print("Comparing nixfmt outputs in parallel...\n")

    failures: list[tuple[Path, str]] = []
    completed = 0

    workers = max(1, os.cpu_count() or 1)
    with ThreadPoolExecutor(max_workers=workers) as executor:
        for path, (success, message) in zip(
            nix_files, executor.map(compare_file, nix_files)
        ):
            if not success:
                failures.append((path, message))
                print(f"❌ {path}: {message}")

            completed += 1
            if completed % 100 == 0:
                print(f"✓ Processed {completed}/{total} files")

    passed = total - len(failures)
    print("\n" + "=" * 60)
    print(f"Files passed: {passed}/{total}")
    if failures:
        print(f"Failures: {len(failures)}")
        sys.exit(1)

    print(f"✅ nixfmt_rs matches nixfmt for all files in nixpkgs ({COMPARE_MODE.upper()} mode)")
    sys.exit(0)


if __name__ == "__main__":
    main()
