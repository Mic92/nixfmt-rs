{
  x = ''
    /*nixfmt:disable*/
    not a directive
  '';
  y   =   1;
  z = "${
/*nixfmt:disable*/
    1    +    2
/*nixfmt:enable*/
  }";
  w   =   3;
}
