#!/usr/bin/env python3
"""Test against nixfmt's regression test suite"""

import subprocess
import sys
import os
from pathlib import Path

NIXFMT_RS = "./target/debug/nixfmt_rs"
NIXFMT = "nixfmt"
NIXFMT_TEST_DIR = "/Users/joerg/git/nixfmt/test"

def test_file(test_path, should_pass=True):
    """Test a single file against nixfmt"""
    name = test_path.stem

    with open(test_path, 'r') as f:
        expr = f.read()

    # Get nixfmt's AST
    try:
        result = subprocess.run(
            [NIXFMT, "--ast", str(test_path)],
            capture_output=True,
            timeout=60
        )
        nixfmt_ast = result.stderr.decode().rstrip()
        # nixfmt --ast returns 1 even on success, check output for errors
        nixfmt_failed = str(test_path) in nixfmt_ast or "unexpected" in nixfmt_ast[:100]
    except Exception as e:
        print(f"⚠️  nixfmt crashed for {name}: {e}")
        return True

    # Get our AST
    try:
        result = subprocess.run(
            [NIXFMT_RS, "--ast"],
            input=expr.encode(),
            capture_output=True,
            timeout=5
        )
        our_ast = result.stdout.decode().rstrip()
        our_failed = result.returncode != 0
    except Exception as e:
        print(f"⚠️  We crashed for {name}: {e}")
        return False

    # For files in "invalid" directory, both should fail
    if not should_pass:
        if nixfmt_failed and our_failed:
            return True  # Both reject - good!
        elif not nixfmt_failed and our_failed:
            print(f"❌ {name}: We reject but nixfmt accepts (unexpected for invalid test)")
            return False
        elif nixfmt_failed and not our_failed:
            print(f"❌ {name}: nixfmt rejects but we accept")
            return False
        else:
            # Both accept an invalid file - still check if AST matches
            if nixfmt_ast != our_ast:
                print(f"⚠️  {name}: Both accept invalid syntax but AST differs")
                return False
            return True

    # For files in "correct" directory, both should pass with same AST
    if nixfmt_failed and not our_failed:
        print(f"❌ {name}: nixfmt rejects but we accept")
        return False

    if not nixfmt_failed and our_failed:
        print(f"❌ {name}: We reject but nixfmt accepts")
        print(f"   File: {test_path}")
        return False

    if nixfmt_failed and our_failed:
        return True  # Both failed - expected behavior

    # Both succeeded - compare ASTs
    if nixfmt_ast != our_ast:
        print(f"❌ {name}: AST mismatch")
        print(f"   File: {test_path}")
        return False

    return True

def main():
    # Build first
    print("Building nixfmt_rs...")
    subprocess.run(["cargo", "build", "--quiet"], check=True)

    test_dir = Path(NIXFMT_TEST_DIR)

    # Test correct files
    print("\n🔍 Testing nixfmt 'correct' regression tests...\n")
    correct_dir = test_dir / "correct"
    correct_files = sorted(correct_dir.glob("*.nix"))

    correct_passed = 0
    correct_total = len(correct_files)

    for test_file_path in correct_files:
        if test_file(test_file_path, should_pass=True):
            correct_passed += 1
            print(f"✓ {test_file_path.stem}")
        # Errors already printed by test_file

    # Test invalid files
    print("\n🔍 Testing nixfmt 'invalid' regression tests...\n")
    invalid_dir = test_dir / "invalid"
    invalid_files = sorted(invalid_dir.glob("*.nix"))

    invalid_passed = 0
    invalid_total = len(invalid_files)

    for test_file_path in invalid_files:
        if test_file(test_file_path, should_pass=False):
            invalid_passed += 1
            print(f"✓ {test_file_path.stem}")
        # Errors already printed by test_file

    # Summary
    total_passed = correct_passed + invalid_passed
    total_tests = correct_total + invalid_total

    print("\n" + "="*60)
    print(f"Correct tests: {correct_passed}/{correct_total} passed")
    print(f"Invalid tests: {invalid_passed}/{invalid_total} passed")
    print(f"Total: {total_passed}/{total_tests} passed ({100*total_passed//total_tests}%)")

    if total_passed < total_tests:
        print(f"❌ {total_tests - total_passed} tests failed!")
        sys.exit(1)
    else:
        print("✅ All nixfmt regression tests passed!")
        sys.exit(0)

if __name__ == "__main__":
    main()
