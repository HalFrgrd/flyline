# Feel free to add your own animations here!


flyline create-anim --name "MY_ANIMATION" --fps 10  ⣾ ⣷ ⣯ ⣟ ⡿ ⢿ ⣻ ⣽

# Now you can use this animation in your prompt. For example:
# PS1='\u@\h:\w MY_ANIMATION'

# This replaces any instance of "john" with the animation, so it will animate every time "john" appears in the prompt.
flyline create-anim --name "john" --ping-pong --fps 8 \
    john \
    john \
    john \
    John \
    jOhn \
    joHn \
    johN \
    john \
    john


# Add styling with ANSI escape codes:
BLACK='\e[30m'
RED='\e[31m'
GREEN='\e[32m'
YELLOW='\e[33m'
BLUE='\e[34m'
MAGENTA='\e[35m'
CYAN='\e[36m'
WHITE='\e[37m'

flyline create-anim --name "john" --ping-pong --fps 3 \
    "${YELLOW}john" \
    "${RED}john" \
    "${MAGENTA}john" \
    "${BLUE}john" \
    "${CYAN}john" \
    "${GREEN}john" \
    "${WHITE}john"


# Experiment with higher frame rates for smooth transitions:

# flyline create-anim --name "john" --ping-pong --fps 30 \
#     '\e[38;2;255;0;0mjohn' \
#     '\e[38;2;223;32;0mjohn' \
