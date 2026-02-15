# Script to be sourced into a bash shell
_zipzap_precmd() {
    local cwd="$(command pwd 2>/dev/null)"
    [[ "$cwd" != "$_ZIPZAP_LAST_DIR" ]] && (zipzap --quiet add "$cwd" &)
    _ZIPZAP_LAST_DIR="$cwd"
}
grep "_zipzap_precmd" <<< "$PROMPT_COMMAND" >/dev/null || {
    if [[ -z "${PROMPT_COMMAND//[[:space:]]/}" ]]; then
        PROMPT_COMMAND='_zipzap_precmd'
    else
        PROMPT_COMMAND="${PROMPT_COMMAND}"$'\n''_zipzap_precmd'
    fi
}
z() {
    local target=$(zipzap --quiet find "$@")
    local Z_STATUS=$?
    if [[ $Z_STATUS -eq 0 && -n "$target" ]]; then
        builtin cd "$target"
    fi
    return $Z_STATUS
}
