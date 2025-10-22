# strace -s  99 -tt -e read,write  bash --init-file behaviours/test_6.sh
bind "set enable-bracketed-paste off"

# echo "myprompt> "
PS1="myprompt> "
# bind -x '"a": printf "\033[?1049h"'
# bind -x '"b": printf "\033[?1049l"'
bind '"a": "becho hello\033[2K"'
bind -x '"b": sleep 1'
# bind -x '"d": sleep 1'
