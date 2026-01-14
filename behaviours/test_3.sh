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

flyline_start_of_prompt() {
    FLYLINE_NEED_RESTORE_OUTPUT=1
    sleep 2
    FLYLINE_COMMAND="ls -l | head"

    bind -x '"a": READLINE_LINE="echo hellow"'
    bind '"j": accept-line'
    bind '"\e[0n": "aj"'

    # https://unix.stackexchange.com/questions/797263/tty-line-discipline-echoing-feature
    stty -echo
    printf "\033[5n"
}

PROMPT_COMMAND='flyline_start_of_prompt'
PS1="MYPROMPT>\n"


flyline_end_of_prompt() {
    READLINE_LINE=${FLYLINE_COMMAND};
    READLINE_POINT=${#READLINE_LINE};

    bind '"a": self-insert'
    bind '"j": self-insert'
    bind -r '\e[0n'
}

flyline_restore_output() {
    if [ -z "$FLYLINE_NEED_RESTORE_OUTPUT" ]; then
        returnp
    fi
    unset FLYLINE_NEED_RESTORE_OUTPUT
    stty echo
}

trap 'flyline_restore_output' DEBUG
