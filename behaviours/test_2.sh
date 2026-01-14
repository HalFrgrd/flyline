# run with `bash --init-file behaviours/test_2.sh`
# like test_1.sh but automated

# output should be:
# MYPROMPT>
# hellow
# MYPROMPT>
# hellow
# MYPROMPT>
# hellow

bind "set enable-bracketed-paste off"

flyline_start_of_prompt() {
    sleep 1
    stty -echo
    printf "\033[5n"
}

PROMPT_COMMAND='flyline_start_of_prompt'
PS1="MYPROMPT>\n"
bind '"\e[0n": "fj"'
bind -x '"f": READLINE_LINE="echo hellow"'
trap 'stty echo' DEBUG
bind '"j": accept-line'
