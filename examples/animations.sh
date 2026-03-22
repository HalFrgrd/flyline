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

flyline create-anim --name "john" --fps 30 \
    '\e[38;2;255;0;0mjohn' \
    '\e[38;2;255;16;0mjohn' \
    '\e[38;2;255;32;0mjohn' \
    '\e[38;2;255;48;0mjohn' \
    '\e[38;2;255;64;0mjohn' \
    '\e[38;2;255;80;0mjohn' \
    '\e[38;2;255;96;0mjohn' \
    '\e[38;2;255;112;0mjohn' \
    '\e[38;2;255;128;0mjohn' \
    '\e[38;2;255;144;0mjohn' \
    '\e[38;2;255;160;0mjohn' \
    '\e[38;2;255;176;0mjohn' \
    '\e[38;2;255;192;0mjohn' \
    '\e[38;2;255;208;0mjohn' \
    '\e[38;2;255;224;0mjohn' \
    '\e[38;2;255;240;0mjohn' \
    '\e[38;2;255;255;0mjohn' \
    '\e[38;2;239;255;16mjohn' \
    '\e[38;2;223;255;32mjohn' \
    '\e[38;2;207;255;48mjohn' \
    '\e[38;2;191;255;64mjohn' \
    '\e[38;2;175;255;80mjohn' \
    '\e[38;2;159;255;96mjohn' \
    '\e[38;2;143;255;112mjohn' \
    '\e[38;2;127;255;128mjohn' \
    '\e[38;2;111;255;144mjohn' \
    '\e[38;2;95;255;160mjohn' \
    '\e[38;2;79;255;176mjohn' \
    '\e[38;2;63;255;192mjohn' \
    '\e[38;2;47;255;208mjohn' \
    '\e[38;2;31;255;224mjohn' \
    '\e[38;2;15;255;240mjohn' \
    '\e[38;2;0;255;255mjohn' \
    '\e[38;2;16;239;255mjohn' \
    '\e[38;2;32;223;255mjohn' \
    '\e[38;2;48;207;255mjohn' \
    '\e[38;2;64;191;255mjohn' \
    '\e[38;2;80;175;255mjohn' \
    '\e[38;2;96;159;255mjohn' \
    '\e[38;2;112;143;255mjohn' \
    '\e[38;2;128;127;255mjohn' \
    '\e[38;2;144;111;255mjohn' \
    '\e[38;2;160;95;255mjohn' \
    '\e[38;2;176;79;255mjohn' \
    '\e[38;2;192;63;255mjohn' \
    '\e[38;2;208;47;255mjohn' \
    '\e[38;2;224;31;255mjohn' \
    '\e[38;2;240;15;255mjohn' \
    '\e[38;2;255;0;255mjohn' \
    '\e[38;2;255;0;239mjohn' \
    '\e[38;2;255;0;223mjohn' \
    '\e[38;2;255;0;207mjohn' \
    '\e[38;2;255;0;191mjohn' \
    '\e[38;2;255;0;175mjohn' \
    '\e[38;2;255;0;159mjohn' \
    '\e[38;2;255;0;143mjohn' \
    '\e[38;2;255;0;127mjohn' \
    '\e[38;2;255;0;111mjohn' \
    '\e[38;2;255;0;95mjohn' \
    '\e[38;2;255;0;79mjohn' \
    '\e[38;2;255;0;63mjohn' \
    '\e[38;2;255;0;47mjohn' \
    '\e[38;2;255;0;31mjohn' \
    '\e[38;2;255;0;15mjohn' 
