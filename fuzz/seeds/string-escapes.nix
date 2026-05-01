{
  a = "x\r\ty\r\n";
  b = ''
    keep ''' triple
    keep ''$ dollar
    keep ''\n newline
  '';
  c = '' one ''' two ''$ three '';
  d = '' ''${x} '';
}
