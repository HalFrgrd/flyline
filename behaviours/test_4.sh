
PROMPT_COMMAND=""

bind "set enable-bracketed-paste off"

# bind '"y": "ls | head -n 5\C-m"'
bind '"z": "kls | head -n 5u\C-m"'
# bind '"j": "k\C-m"'
bind -x '"k": printf "\033[?1049h" && echo qwe  &&  sleep 3'
# bind -x '"k": printf "\033[?1049h" && sleep 1'
# bind -x '"u": sleep 3 &&  echo asdf && sleep 4 && printf "\033[?1049l"'
# bind -x '"u": sleep 3 && printf "\033[?1049l"'
bind '"u": "\e[31mHello\e[0m"'
bind '"u": "kHello"'
bind -x '"k": sleep 1'