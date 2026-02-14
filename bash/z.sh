# Script to be sourced into a bash shell
grep "zipzap" <<< "$PROMPT_COMMAND" >/dev/null || {
    PROMPT_COMMAND="$PROMPT_COMMAND"$'\n''(zipzap --quiet add "$(command pwd 2>/dev/null)"&);'
}
z() {
    local target
    target=$(zipzap --quiet find "$@")
    local Z_STATUS=$?
    if [[ $Z_STATUS -eq 0 && -n "$target" ]]; then
        builtin cd "$target"
    fi
    return $Z_STATUS
}
