let
  x = { or = 1; a = 2; };
in [
  x.or
  (x.a or 99)
  ({ or = 1; })
  (x.or or x.or)
]
