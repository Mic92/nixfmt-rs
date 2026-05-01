{
  a = "${[ 1 2 3 ]}";
  b = "x${{ k = v; }}y";
  c = "${(foo)}";
  d = ''
${foo bar baz quux}
    ${foo bar baz quux}
    text ${foo bar baz quux} more
  '';
  e = ''
no indent at all
  '';
  f = ''
  '';
  g = ''  '';
  h = '''';
}
