# bash completion for nixfmt
_nixfmt() {
  local cur prev
  COMPREPLY=()
  cur=${COMP_WORDS[COMP_CWORD]}
  prev=${COMP_WORDS[COMP_CWORD - 1]}

  case "$prev" in
    --message-format)
      mapfile -t COMPREPLY < <(compgen -W "human json" -- "$cur")
      return
      ;;
    -w | --width | --indent | -f | --filename)
      return
      ;;
  esac

  if [[ $cur == -* ]]; then
    local opts="-w --width --indent -c --check -m --mergetool -q --quiet \
      -s --strict -v --verify -a --ast --ir -f --filename \
      --message-format -h --help -V --version --numeric-version"
    mapfile -t COMPREPLY < <(compgen -W "$opts" -- "$cur")
    return
  fi

  compopt -o filenames 2>/dev/null
  mapfile -t COMPREPLY < <(
    compgen -f -X '!*.nix' -- "$cur"
    compgen -d -- "$cur"
  )
}
complete -F _nixfmt nixfmt
