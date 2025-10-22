# run with `bash --init-file behaviours/test_3.sh`
# like test_2.sh but in functions

# output should be:
# MYPROMPT>
# hellow
# MYPROMPT>
# hellow
# MYPROMPT>
# hellow

bind "set enable-bracketed-paste off"

jobu_start_of_prompt() {
    JOBU_NEED_RESTORE_OUTPUT=1
    sleep 2
    JOBU_COMMAND="ls -l | head"

    bind -x '"a": READLINE_LINE="echo hellow"'
    bind '"j": accept-line'
    bind '"\e[0n": "aj"'

    stty -echo
    printf "\033[5n"
}

PROMPT_COMMAND='jobu_start_of_prompt'
PS1="MYPROMPT>\n"


jobu_end_of_prompt() {
    READLINE_LINE=${JOBU_COMMAND};
    READLINE_POINT=${#READLINE_LINE};

    bind '"a": self-insert'
    bind '"j": self-insert'
    bind -r '\e[0n'
}

jobu_restore_output() {
    if [ -z "$JOBU_NEED_RESTORE_OUTPUT" ]; then
        returnp
    fi
    unset JOBU_NEED_RESTORE_OUTPUT
    stty echo
}

trap 'jobu_restore_output' DEBUG
