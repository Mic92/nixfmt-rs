{
  a = {
    /*nixfmt:disable*/
    b    =    1;
    /*nixfmt:enable*/
  };
  c = [
    /*nixfmt:disable*/
    "x"    "y"    "z"
    /*nixfmt:enable*/
  ];
  d = let
    /*nixfmt:disable*/
    e    =    1;
    /*nixfmt:enable*/
  in e;
}
