{
  a # name
  ,
  b ? 1 # default
  ,
  c ? a.${b} # sel-interp
  ,
  d ? a."s" # sel-string
  ,
  e ? ./p # path
  ,
  f ? with x; y # with
  ,
  g ? let z = 1; in z # let
  ,
  h ? -1 # neg
  ,
  i ? x: x # abs
  ,
  j ? a ? b # member
  ,
  ...
}@args:
a
