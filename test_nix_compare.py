#!/usr/bin/env python3
"""Compare nixfmt_rs parser against nix-instantiate --parse across the Nix repository test suite.

This script walks the Nix repository checkout located next to this repository,
parses every *.nix test file via both nixfmt_rs (--ast) and nix-instantiate --parse,
and reports any mismatches in parse success/failure. The work is parallelised to speed up runs.
"""

import os
import sys
import subprocess
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import Tuple


NIXFMT_RS = "./target/debug/nixfmt_rs"
NIX_INSTANTIATE = "nix-instantiate"
REPO_ROOT = Path(__file__).parent.resolve()
NIX_ROOT = (REPO_ROOT / "../nix").resolve()
MAX_TIMEOUT = 120  # seconds per invocation


def compare_file(path: Path) -> Tuple[bool, str]:
    """Return (success, message) for a single file comparison."""
    try:
        expr = path.read_bytes()
    except Exception as exc:
        return False, f"failed to read file: {exc}"

    # Check if nix-instantiate --parse can parse the file
    try:
        nix_proc = subprocess.run(
            [NIX_INSTANTIATE, "--parse", str(path)],
            capture_output=True,
            timeout=MAX_TIMEOUT,
            check=False,
        )
    except FileNotFoundError:
        return False, "nix-instantiate executable not found in PATH"
    except subprocess.TimeoutExpired:
        return False, "nix-instantiate timed out"

    nix_failed = nix_proc.returncode != 0

    # Check if nixfmt_rs can parse the file
    try:
        ours_proc = subprocess.run(
            [NIXFMT_RS, "--ast"],
            input=expr,
            capture_output=True,
            timeout=MAX_TIMEOUT,
            check=False,
        )
    except FileNotFoundError:
        return False, "build artefact nixfmt_rs not found – run `cargo build` first"
    except subprocess.TimeoutExpired:
        return False, "nixfmt_rs timed out"

    our_failed = ours_proc.returncode != 0

    # Compare parse success/failure
    if nix_failed and not our_failed:
        nix_stderr = nix_proc.stderr.decode("utf-8", errors="replace").rstrip()
        return False, f"nix-instantiate fails to parse but nixfmt_rs accepts\n{nix_stderr[:200]}"

    if not nix_failed and our_failed:
        ours_stderr = ours_proc.stderr.decode("utf-8", errors="replace").rstrip()
        return False, f"nixfmt_rs fails to parse but nix-instantiate accepts\n{ours_stderr[:200]}"

    # Both succeed or both fail - this is good
    return True, ""


def gather_files(root: Path) -> list[Path]:
    if not root.exists():
        raise FileNotFoundError(f"Nix checkout not found at {root}")
    # Focus on the test directories
    return [p for p in root.rglob("*.nix") if p.is_file() and "tests/" in str(p)]


def main() -> None:
    print("Building nixfmt_rs...")
    subprocess.run(["cargo", "build", "--quiet"], check=True)

    try:
        nix_files = gather_files(NIX_ROOT)
    except FileNotFoundError as exc:
        print(f"❌ {exc}")
        sys.exit(1)

    total = len(nix_files)
    if total == 0:
        print(f"No .nix test files found under {NIX_ROOT}")
        sys.exit(0)

    print(f"Found {total} .nix test files under {NIX_ROOT}")
    print("Comparing parse results (nix-instantiate vs nixfmt_rs) in parallel...\n")

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
            if completed % 50 == 0:
                print(f"✓ Processed {completed}/{total} files")

    passed = total - len(failures)
    print("\n" + "=" * 60)
    print(f"Files passed: {passed}/{total}")
    if failures:
        print(f"Failures: {len(failures)}")
        print("\nFailed files:")
        for path, msg in failures:
            print(f"  {path.relative_to(NIX_ROOT)}")
        sys.exit(1)

    print("✅ nixfmt_rs parse results match nix-instantiate for all test files in Nix repository")
    sys.exit(0)


if __name__ == "__main__":
    main()
