echo 'hello from jobu'
# PROMPT_COMMAND='echo $LINENO > $JOBU_FIFO_PATH 2>/dev/null || true'


# edit-and-execute-command annoyingly echos the command before executing it.
# so i need to delete that line from the terminal in debug trap.

# behaviour:
# - lines with no command not print the command
# - setting `bind '"\C-m": edit-and-execute-command'`  will echo the command
# - multiline editing is not supported with `edit-and-execute-command`, it will always think you want to execute after pressing enter.
# - so we'd have to swap what enter does depending on context.
# so I don't see what using `edit-and-execute-command` gets me.
# because alternatively, I could either set enter to accept-line, or to send to daemon.
# and then I would not have to deal with the echoed command.
# the only benefit I see with `edit-and-execute-command` is that I get to run a function before executing the command.
# but I could also do that with a DEBUG trap. But maybe it is best not to touch the debug trap.
# 
# well `bind '"\C-m": accept-line'` tries to do mulitiline editing, which we might not want.


# TODO: execute-named-command seems interesting, but not available in my bash version
# insert-comment could be useful also. it also accepts commands


# JOBU_SHOULD_DELETE_ECHOED_COMMAND=0
# dumb_editor() {
#     f="$1"
#     # echo hello
#     [ -z "$f" ] && return 1
#     [ ! -e "$f" ] && : > "$f"
#     content=$(sed -n '1p' -- "$f" 2>/dev/null || printf '')
#     if [ "$content" = "exit" ]; then
#         return 0
#     fi
#     # printf '%s' 'ls | head -n 2' > "$f"
#     JOBU_SHOULD_DELETE_ECHOED_COMMAND=1

#     return 0
# }

# get_rid_of_echoed_command() {
#     # echo "in get_rid_of_echoed_command JOBU_SHOULD_DELETE_ECHOED_COMMAND=$JOBU_SHOULD_DELETE_ECHOED_COMMAND"
#     # Clear the last line in the terminal
#     [ "${JOBU_SHOULD_DELETE_ECHOED_COMMAND:-0}" -ne 1 ] && return
#     tput cuu1  # Move cursor up one line
#     tput el    # Clear to the end of the line
#     JOBU_SHOULD_DELETE_ECHOED_COMMAND=0
# }

# trap 'get_rid_of_echoed_command' DEBUG


# testswitch(){
#     echo "switching j"
#     # bind '"j": self-insert'
#     READLINE_LINE='echo "test'
#     READLINE_POINT=${#READLINE_LINE}
# }

# export VISUAL=dumb_editor
# bind '"\C-m": accept-line'
# bind '"j": execute-named-command'
# bind '"j": operate-and-get-next'


# bind '"\C-m": edit-and-execute-command'
# bind -x '"\C-m": "edit-and-execute-command\n"'

bind '"q": "z\C-J"'
bind '"z": "exit"'

jobu_start_of_prompt() {
    echo "starting prompt"
    export JOBU_COMMAND="sleep 2 && ls -l | head -n 2"
    
    bind -x '"j": jobu_end_of_prompt'
    bind '"\e[0n": "j\C-J"'
    # bind -x '"\e[0n": echo "received"'
    echo -en "\033[5n"
}

PROMPT_COMMAND='jobu_start_of_prompt'

jobu_end_of_prompt() {
    echo "ending prompt function"
    READLINE_LINE=${JOBU_COMMAND};
    READLINE_POINT=${#READLINE_LINE}; 
    echo "READLINE_LINE is now: $READLINE_LINE"
    bind '"j": self-insert'
    bind -r '\e[0n'
}