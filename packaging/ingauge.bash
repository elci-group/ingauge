_ingauge() {
    local commands="status providers discover probe history forecast next health daemon config db export help"
    if [[ ${COMP_CWORD} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "$commands --json --config --help --version" -- "${COMP_WORDS[COMP_CWORD]}") )
    fi
}
complete -F _ingauge ingauge
