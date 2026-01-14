
PS1="myprompt> ooutput of flyline\n"


setup_for_readline() {
    READLINE_LINE="ls -l | head -n 5"
    READLINE_POINT=${#READLINE_LINE}
    printf "\033[?1049h"   # hihi, readline won't know it is writing to the alternate buffer!
}


bind '"j": "k\C-m"'
bind -x '"k": setup_for_readline'
# after readline is done printing PS1, echoing the command after setup_for_readline runs, 
# PS0 will be printed and we'll go back to the normal buffer
# readline does everything it wants to do in the alternate buffer and doesn't mess up
# the lovely output flyline has prepared for us
PS0="\033[?1049l" 

# and because readline uses the alternate buffer,
# this setting doesn't matter any more!
# bind "set enable-bracketed-paste off"
