_zipzap_precmd() {
    local cwd="$(command pwd 2>/dev/null)"
    [[ "$cwd" != "$_ZIPZAP_LAST_DIR" ]] && (zipzap --quiet add "$cwd" &)
    _ZIPZAP_LAST_DIR="$cwd"
}
(( ${precmd_functions[(I)_zipzap_precmd]} )) || precmd_functions+=(_zipzap_precmd)
z() {
    local target=$(zipzap --quiet find "$@")
    local Z_STATUS=$?
    if [[ $Z_STATUS -eq 0 && -n "$target" ]]; then
        builtin cd "$target"
    fi
    return $Z_STATUS
}
