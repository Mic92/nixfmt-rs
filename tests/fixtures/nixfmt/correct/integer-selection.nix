# Selecting an attribute on a numeric literal must keep a separating space:
# `1.a` re-lexes as the float `1.` applied to `a`, which is a different
# program. The space below is therefore load-bearing.
{
  a = 1 .a;
  b = 42 .foo.bar or 0;
  c = 1.5 .x;
}
