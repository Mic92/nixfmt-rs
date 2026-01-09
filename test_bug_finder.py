#!/usr/bin/env python3
"""Systematic bug finder - tests various Nix expressions against nixfmt"""

import subprocess
import sys
import difflib
import tempfile
import os

NIXFMT_RS = "./target/debug/nixfmt_rs"
NIXFMT = "nixfmt"

def test_expression(name, expr):
    """Test a single expression, return True if passes"""

    # Get nixfmt's AST (outputs to stderr, requires file input)
    try:
        with tempfile.NamedTemporaryFile(mode='w', suffix='.nix', delete=False) as f:
            f.write(expr)
            temp_file = f.name

        try:
            result = subprocess.run(
                [NIXFMT, "--ast", temp_file],
                capture_output=True,
                timeout=5
            )
            nixfmt_ast = result.stderr.decode().rstrip()
            # nixfmt --ast always returns exit code 1, check output instead
            # Error messages start with "<stdin>:" or the filename
            nixfmt_failed = nixfmt_ast.startswith(temp_file) or "unexpected" in nixfmt_ast[:100]
        finally:
            os.unlink(temp_file)
    except Exception as e:
        print(f"⚠️  Warning: nixfmt crashed for {name}: {e}")
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
        print(f"❌ BUG: {name} - Our parser crashed: {e}")
        print(f"   Expression: {expr}")
        return False

    # Check for mismatch in success/failure
    if nixfmt_failed and not our_failed:
        print(f"❌ BUG: {name} - nixfmt rejects but we accept")
        print(f"   Expression: {expr}")
        print(f"   nixfmt error: {nixfmt_ast[:200]}")
        return False

    if not nixfmt_failed and our_failed:
        print(f"❌ BUG: {name} - We reject but nixfmt accepts")
        print(f"   Expression: {expr}")
        print(f"   Our error: {our_ast[:200]}")
        return False

    # Both failed - that's fine
    if nixfmt_failed and our_failed:
        return True

    # Both succeeded - compare ASTs
    if nixfmt_ast != our_ast:
        print(f"❌ BUG: {name} - AST mismatch")
        print(f"   Expression: {expr}")

        # Show unified diff
        diff = list(difflib.unified_diff(
            nixfmt_ast.splitlines(keepends=True),
            our_ast.splitlines(keepends=True),
            fromfile='nixfmt',
            tofile='ours',
            lineterm=''
        ))

        if diff:
            print("   Diff (first 20 lines):")
            for line in diff[:20]:
                print(f"   {line.rstrip()}")

        return False

    return True

def main():
    # Build first
    print("Building nixfmt_rs...")
    subprocess.run(["cargo", "build", "--quiet"], check=True)

    print("\n🔍 Searching for bugs...\n")

    # Disabled the original exhaustive suite to keep runs fast while we focus on the
    # string-selector regression. Keeping it commented for quick reactivation.
    # full_test_cases = [
    #     # Basic expressions
    #     ("integer", "42"),
    #     ("negative_int", "-42"),
    #     ("float", "3.14"),
    #     ("negative_float", "-3.14"),
    #     ("string", '"hello"'),
    #     ("path", "./foo"),
    #     ("home_path", "~/foo"),
    #     ("absolute_path", "/foo/bar"),
    #     ("angle_path", "<nixpkgs>"),
    #
    #     # Functions
    #     ("simple_lambda", "x: x"),
    #     ("lambda_with_pattern", "{x}: x"),
    #     ("lambda_with_default", "{x ? 1}: x"),
    #     ("lambda_with_ellipsis", "{x, ...}: x"),
    #     ("lambda_at_pattern", "x@{y}: x"),
    #     ("lambda_pattern_at", "{x}@y: x"),
    #     ("lambda_chain", "x: y: x"),
    #     ("lambda_multi_default", "{x ? 1, y ? 2}: x"),
    #
    #     # Let expressions
    #     ("let_simple", "let x = 1; in x"),
    #     ("let_multiple", "let x = 1; y = 2; in x"),
    #     ("let_inherit", "let inherit x; in x"),
    #     ("let_inherit_from", "let inherit (x) y; in y"),
    #     ("let_string_key", 'let "foo" = 1; in foo'),
    #     ("let_interp_key", 'let ${"foo"} = 1; in foo'),
    #
    #     # With expressions
    #     ("with_simple", "with x; y"),
    #     ("with_nested", "with x; with y; z"),
    #
    #     # Assert
    #     ("assert_simple", "assert true; 1"),
    #     ("assert_nested", "assert x; assert y; z"),
    #
    #     # If-then-else
    #     ("if_simple", "if true then 1 else 2"),
    #     ("if_nested", "if a then if b then 1 else 2 else 3"),
    #
    #     # Operations - edge cases
    #     ("negation", "-1"),
    #     # ("double_negation", "- -1"),  # KNOWN BUG #1
    #     # ("double_negation_no_space", "--1"),  # KNOWN BUG #1
    #     ("negation_paren", "-(1)"),
    #     ("negation_app", "-f x"),
    #     ("not", "!true"),
    #     # ("double_not", "!!true"),  # KNOWN BUG #2
    #     ("not_paren", "!(true)"),
    #
    #     # Binary operations - precedence edge cases
    #     ("mul_div", "1 * 2 / 3"),
    #     ("add_mul", "1 + 2 * 3"),
    #     ("mul_add", "1 * 2 + 3"),
    #     ("sub_add", "1 - 2 + 3"),
    #     # ("add_sub", "1 + 2 - 3"),  # KNOWN BUG #4
    #     ("div_mul", "1 / 2 * 3"),
    #
    #     # Comparison chains (these should fail in Nix - non-associative)
    #     ("comparison_lt", "1 < 2"),
    #     # ("comparison_chain", "1 < 2 < 3"),  # KNOWN BUG #3
    #     ("comparison_mixed", "1 < 2 == true"),
    #
    #     # Logical operators
    #     ("equality", "1 == 2"),
    #     ("inequality", "1 != 2"),
    #     ("and_or", "true && false || true"),
    #     ("or_and", "true || false && true"),
    #     ("implies", "true -> false"),
    #     ("implies_chain", "a -> b -> c"),
    #
    #     # Update and concat
    #     ("update", "{} // {}"),
    #     ("update_chain", "{} // {} // {}"),
    #     ("concat", "[] ++ []"),
    #     ("concat_chain", "[] ++ [] ++ []"),
    #
    #     # Application edge cases
    #     ("app_simple", "f x"),
    #     ("app_multiple", "f x y"),
    #     ("app_with_op", "f x + y"),
    #     ("app_nested", "f (g x)"),
    #     ("app_chain", "f x y z"),
    #
    #     # Selection edge cases
    #     ("select_simple", "a.b"),
    #     ("select_chain", "a.b.c"),
    #     ("select_default", "a.b or 1"),
    #     ("select_default_nested", "a.b.c or 1"),
    #     ("select_app", "f x .y"),
    #     ("select_op", "a.b + c.d"),
    #     ("select_string_attr", 'x."y"'),
    #     ("select_string_interp_attr", 'x.${"y"}'),
    #
    #     # Member check edge cases
    #     ("member_check", "a ? b"),
    #     ("member_check_chain", "a ? b.c"),
    #     ("member_check_multi", "a ? b ? c"),
    #
    #     # Lists
    #     ("list_empty", "[]"),
    #     ("list_single", "[1]"),
    #     ("list_multiple", "[1 2 3]"),
    #     ("list_nested", "[[1] [2]]"),
    #     ("list_mixed", "[1 [2] 3]"),
    #
    #     # Sets
    #     ("set_empty", "{}"),
    #     ("set_simple", "{a=1;}"),
    #     ("set_multiple", "{a=1; b=2;}"),
    #     ("set_nested", "{a={b=1;};}"),
    #     ("set_rec", "rec {a=1;}"),
    #     ("set_inherit", "{inherit a;}"),
    #     ("set_inherit_from", "{inherit (x) a;}"),
    #     ("set_string_key", '{"a" = 1;}'),
    #     ("set_interp_key", '{${"a"} = 1;}'),
    #     ("set_inherit_string", '{inherit "a";}'),
    #     ("set_inherit_interp", '{inherit ${"a"};}'),
    #
    #     # Strings - basic
    #     # ("string_empty", '""'),  # KNOWN BUG #5
    #     ("string_simple", '"hello"'),
    #     ("string_space", '" "'),
    #     ("string_spaces", '"   "'),
    #     ("string_newline_escape", '"\\n"'),
    #     ("string_tab_escape", '"\\t"'),
    #     ("string_quote_escape", '"\\\""'),
    #     ("string_backslash_escape", '"\\\\"'),
    #     ("string_dollar_escape", '"\\$"'),
    #     ("string_all_escapes", '"\\n\\t\\r\\\"\\\\"'),
    #
    #     # String interpolation - simple
    #     ("string_interp", '"${x}"'),
    #     ("string_interp_start", '"${x}hello"'),
    #     ("string_interp_end", '"hello${x}"'),
    #     ("string_interp_middle", '"hello${x}world"'),
    #     ("string_multi_interp", '"${x}${y}"'),
    #     ("string_multi_interp_text", '"${x}text${y}"'),
    #     ("string_interp_complex", '"a${b}c${d}e"'),
    #
    #     # String interpolation - nested
    #     ("string_interp_nested", '"${a.b}"'),
    #     ("string_interp_op", '"${1 + 2}"'),
    #     ("string_interp_call", '"${f x}"'),
    #     ("string_interp_lambda", '"${x: x}"'),
    #     ("string_interp_let", '"${let x = 1; in x}"'),
    #     ("string_interp_string", '"\\"${"\\"hello\\""}\\""'),
    #     ("string_interp_if", '"${if true then 1 else 2}"'),
    #     ("string_interp_with", '"${with x; y}"'),
    #     ("string_interp_assert", '"${assert true; 1}"'),
    #     ("string_interp_list", '"${[1 2 3]}"'),
    #     ("string_interp_set", '"${{a=1;}}"'),
    #     ("string_interp_rec_set", '"${rec {a=1;}}"'),
    #     ("string_interp_paren", '"${(1 + 2) * 3}"'),
    #     ("string_interp_nested_deep", '"${a.b.c.d}"'),
    #     ("string_interp_op_chain", '"${1 + 2 * 3 / 4}"'),
    #     ("string_interp_app_chain", '"${f x y z}"'),
    #     ("string_interp_update", '"${{} // {a=1;}}"'),
    #     ("string_interp_concat", '"${[] ++ [1]}"'),
    #     ("string_interp_member_check", '"${a ? b}"'),
    #     ("string_interp_lambda_app", '"${(x: x) 1}"'),
    #     ("string_interp_nested_lambda", '"${x: y: x}"'),
    #     ("string_interp_pattern", '"${{x, y}: x}"'),
    #     ("string_interp_pattern_default", '"${{x ? 1}: x}"'),
    #     ("string_interp_at_pattern", '"${x@{y}: x}"'),
    #
    #     # String interpolation - edge cases
    #     ("string_dollar_dollar", '"$$"'),
    #     ("string_dollar_space", '"$ "'),
    #     ("string_dollar_text", '"$x"'),
    #     ("string_empty_interp", '"${}"'),
    #     ("string_interp_whitespace", '"${ x }"'),
    #
    #     # Indented strings - basic
    #     ("indented_string", "''hello''"),
    #     ("indented_multiline", "''hello\\nworld''"),
    #     ("indented_empty", "''"),
    #     # ("indented_space", "'' ''"),  # Same as BUG #5 - formatting diff
    #     ("indented_newline", "''\\n''"),
    #
    #     # Indented strings - interpolation
    #     ("indented_interp", "''${x}''"),
    #     ("indented_interp_start", "''${x}hello''"),
    #     ("indented_interp_end", "''hello${x}''"),
    #     ("indented_interp_middle", "''hello${x}world''"),
    #     ("indented_multi_interp", "''${x}${y}''"),
    #
    #     # Indented strings - escapes
    #     ("indented_dollar_dollar", "''$$''"),
    #     ("indented_quote_escape", "''\\'''"),
    #     ("indented_dollar_escape", "''\\${x}''"),
    #     ("indented_backslash", "''\\\\''"),
    #
    #     # Indented strings - edge cases
    #     ("indented_actual_newline", "''hello\nworld''"),
    #     ("indented_tabs", "''\\t\\t''"),
    #     ("indented_mixed", "''text${x}more\\nend''"),
    #
    #     # Parentheses edge cases
    #     ("paren_simple", "(1)"),
    #     ("paren_nested", "((1))"),
    #     ("paren_op", "(1 + 2) * 3"),
    #
    #     # Complex combinations
    #     ("lambda_let", "x: let y = 1; in x"),
    #     ("let_if", "let x = 1; in if x then 2 else 3"),
    #     ("with_let", "with x; let y = 1; in y"),
    #     ("app_lambda", "(x: x) 1"),
    #     ("select_lambda", "(x: x.y) z"),
    #
    #     # Whitespace edge cases
    #     ("no_space_app", "f(x)"),
    #     ("no_space_op", "1+2"),
    #     ("multi_space", "1  +  2"),
    #
    #     # Comments
    #     ("line_comment", "# comment\n1"),
    #     ("block_comment", "/* comment */ 1"),
    #     ("nested_block_comment", "/* /* nested */ */ 1"),
    #
    #     # Unbalanced/malformed code (should all fail)
    #     ("unbalanced_paren_open", "(1"),
    #     ("unbalanced_paren_close", "1)"),
    #     ("unbalanced_brace_open", "{a=1;"),
    #     ("unbalanced_brace_close", "a=1;}"),
    #     ("unbalanced_bracket_open", "[1"),
    #     ("unbalanced_bracket_close", "1]"),
    #     ("missing_then", "if true else 2"),
    #     ("missing_else", "if true then 1"),
    #     ("missing_in", "let x = 1;"),
    #     ("missing_semicolon", "let x = 1 in x"),
    #     ("missing_colon", "x x"),
    #     ("missing_eq", "{a 1;}"),
    #     ("trailing_op", "1 +"),
    #     ("leading_op", "+ 1"),
    #     ("double_op", "1 + + 2"),
    #     ("empty_paren", "()"),
    #     ("empty_let", "let in x"),
    #     ("empty_if_cond", "if then 1 else 2"),
    #     ("incomplete_string", '"hello'),
    #     ("incomplete_indented", "''hello"),
    #     ("incomplete_interp", '"${'),
    #     ("incomplete_comment", "/* comment"),
    #     ("just_operator", "+"),
    #     ("just_keyword", "let"),
    #     ("just_semicolon", ";"),
    #     ("double_semicolon", "1;;2"),
    #     ("missing_lambda_body", "x:"),
    #     ("missing_pattern_body", "{x}:"),
    #     ("unclosed_set", "{"),
    #     ("unclosed_list", "["),
    #     ("mismatched_delim", "[}"),
    #     ("mismatched_delim2", "{]"),
    #     ("double_dot", "a..b"),
    #     ("trailing_dot", "a."),
    #     ("leading_dot", ".b"),
    #     ("double_question", "a??b"),
    #     ("trailing_question", "a?"),
    #     ("double_at", "x@@y"),
    #     ("trailing_at", "x@"),
    #     ("double_ellipsis", "..."),
    #     # ("orphan_or", "or"),  # KNOWN BUG #6
    #     ("invalid_inherit", "inherit"),
    #     ("inherit_no_semi", "{inherit a}"),
    # ]
    test_cases = [
        # String keys/selectors (current focus)
        ("let_string_key", 'let "foo" = 1; in foo'),
        ("let_interp_key", 'let ${"foo"} = 1; in foo'),
        ("let_interp_expr_key", 'let ${foo} = 1; in foo'),
        ("select_string_attr", 'x."y"'),
        ("select_string_interp_attr", 'x.${"y"}'),
        ("select_interp_expr_attr", 'x.${foo}'),
        ("select_string_attr_default", 'x."y" or 1'),
        ("set_string_key", '{"a" = 1;}'),
        ("set_interp_key", '{${"a"} = 1;}'),
        ("set_interp_expr_key", '{${foo} = 1;}'),
        ("set_inherit_string", '{inherit "a";}'),
        ("set_inherit_interp", '{inherit ${"a"};}'),
        ("set_inherit_string_from", '{inherit (foo) "bar";}'),
        ("set_inherit_multi_string", '{inherit "a" "b";}'),

        # Number literal edge cases (NEW)
        ("sci_notation_basic", "1e10"),
        ("sci_notation_negative_exp", "1.5e-3"),
        ("sci_notation_positive_exp", "1.5e+3"),
        ("sci_notation_capital", "1E10"),
        ("sci_notation_capital_pos", "1.5E+3"),
        ("sci_notation_large", "2.5e100"),
        ("float_zero", "0.0"),
        ("float_leading_zeros", "00.00"),
        ("float_many_decimals", "3.14159265359"),
        ("integer_zero", "0"),
        ("integer_leading_zeros", "007"),
        ("integer_large", "999999999999"),
        ("negative_float_sci", "-1.5e-3"),
        ("negative_zero", "-0"),
        # Invalid numbers (should fail)
        ("float_no_leading", ".5"),
        ("float_no_trailing", "5."),
        ("sci_no_exp_digits", "1e"),
        ("double_decimal", "1.5.5"),
        ("double_sci", "1e10e10"),

        # Operator stacking edge cases (NEW)
        ("mixed_unary_not_neg", "!-1"),
        ("mixed_unary_neg_not", "-!true"),
        ("triple_not", "!!!true"),
        ("triple_neg", "---1"),
        ("unary_paren_not", "!(!(true))"),
        ("unary_paren_neg", "-(-(1))"),
        ("not_neg_var", "!-x"),
        ("neg_not_var", "-!x"),
        ("not_app", "!f x"),
        ("neg_selection", "-a.b"),
        ("not_member_check", "!a ? b"),

        # Comparison operator chains (NEW)
        ("eq_chain", "a == b == c"),
        ("neq_chain", "a != b != c"),
        ("mixed_eq_neq", "a == b != c"),
        ("lt_gt", "a < b > c"),
        ("le_ge", "a <= b >= c"),
        ("mixed_cmp_eq", "a < b == true"),
        ("eq_then_lt", "a == b < c"),

        # Path edge cases (NEW)
        ("angle_path_with_slash", "<nixpkgs/lib>"),
        ("angle_path_deep", "<nixpkgs/lib/strings>"),
        ("path_parent", "./../../foo"),
        ("path_many_dots", "./../../../bar"),
        ("path_with_dash", "./foo-bar"),
        ("path_with_underscore", "./foo_bar"),
        ("path_starting_number", "./123"),
        ("path_just_dot", "."),
        ("path_just_dotdot", ".."),
        # Invalid paths (should fail)
        ("path_empty_angle", "<>"),
        ("path_raw_current_dir", "./"),
        ("path_raw_home_dir", "~/"),
        ("path_trailing_slash_rel", "./foo/"),
        ("path_trailing_slash_home", "~/foo/"),

        # 'or' keyword context tests (NEW)
        # ("or_standalone", "or"),  # KNOWN BUG #6
        ("or_in_let", "let or = 1; in or"),
        ("or_as_param", "{or}: or"),
        ("or_as_func", "or x y"),
        ("or_in_list", "[or]"),
        ("or_in_set", "{or = 1;}"),
        ("or_inherit", "{inherit or;}"),
        ("or_in_app", "f or"),

        # Inherit variations (NEW)
        ("inherit_empty", "{inherit;}"),
        ("inherit_multiple", "{inherit a b c;}"),
        ("inherit_from_empty", "{inherit (x);}"),
        ("inherit_from_multiple", "{inherit (x) a b c;}"),
        ("let_inherit_empty", "let inherit; in x"),
        ("let_inherit_multiple", "let inherit a b c; in x"),
        ("let_inherit_from_empty", "let inherit (x); in x"),
        ("let_inherit_from_multiple", "let inherit (x) a b c; in x"),

        # Whitespace edge cases (NEW)
        ("windows_line_ending", "1\r\n2"),
        ("multiple_newlines", "1\n\n\n2"),
        ("tabs_between_tokens", "1\t+\t2"),
        ("mixed_whitespace", "1  \t  +  \t  2"),
        ("trailing_whitespace", "42   "),
        ("leading_whitespace", "   42"),

        # Comments in various positions (NEW)
        ("comment_eof", "42 # comment"),
        ("multi_line_comments", "1 # one\n+ # plus\n2 # two"),
        ("block_comment_multiline", "/* line1\n   line2\n   line3 */ 1"),
        ("comment_only_file", "# just a comment"),
        ("multiple_block_comments", "/* a */ 1 /* b */ + /* c */ 2"),
    ]

    failed = 0
    total = 0

    for name, expr in test_cases:
        total += 1
        if not test_expression(name, expr):
            failed += 1
            print()  # Extra line between failures

    print("\n" + "="*40)
    print(f"Results: {total - failed}/{total} passed")
    if failed > 0:
        print(f"❌ Found {failed} potential bugs!")
        sys.exit(1)
    else:
        print("✅ No bugs found in basic tests")
        sys.exit(0)

if __name__ == "__main__":
    main()
