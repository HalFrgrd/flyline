# Feel free to add your own animations here!


flyline create-prompt-widget animation --name "MY_ANIMATION" --fps 10  ⣾ ⣷ ⣯ ⣟ ⡿ ⢿ ⣻ ⣽

# Now you can use this animation in your prompt. For example:
# PS1='\u@\h:\w MY_ANIMATION'

# This replaces any instance of "john" with the animation, so it will animate every time "john" appears in the prompt.
flyline create-prompt-widget animation --name "john" --ping-pong --fps 8 \
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

flyline create-prompt-widget animation --name "john" --ping-pong --fps 3 \
    "${YELLOW}\u" \
    "${RED}\u" \
    "${MAGENTA}\u" \
    "${BLUE}\u" \
    "${CYAN}\u" \
    "${GREEN}\u" \
    "${WHITE}\u"


# Experiment with higher frame rates for smooth transitions:
flyline create-prompt-widget animation --name "john" --fps 30 \
    '\e[38;2;255;0;0m\u' \
    '\e[38;2;255;16;0m\u' \
    '\e[38;2;255;32;0m\u' \
    '\e[38;2;255;48;0m\u' \
    '\e[38;2;255;64;0m\u' \
    '\e[38;2;255;80;0m\u' \
    '\e[38;2;255;96;0m\u' \
    '\e[38;2;255;112;0m\u' \
    '\e[38;2;255;128;0m\u' \
    '\e[38;2;255;144;0m\u' \
    '\e[38;2;255;160;0m\u' \
    '\e[38;2;255;176;0m\u' \
    '\e[38;2;255;192;0m\u' \
    '\e[38;2;255;208;0m\u' \
    '\e[38;2;255;224;0m\u' \
    '\e[38;2;255;240;0m\u' \
    '\e[38;2;255;255;0m\u' \
    '\e[38;2;239;255;16m\u' \
    '\e[38;2;223;255;32m\u' \
    '\e[38;2;207;255;48m\u' \
    '\e[38;2;191;255;64m\u' \
    '\e[38;2;175;255;80m\u' \
    '\e[38;2;159;255;96m\u' \
    '\e[38;2;143;255;112m\u' \
    '\e[38;2;127;255;128m\u' \
    '\e[38;2;111;255;144m\u' \
    '\e[38;2;95;255;160m\u' \
    '\e[38;2;79;255;176m\u' \
    '\e[38;2;63;255;192m\u' \
    '\e[38;2;47;255;208m\u' \
    '\e[38;2;31;255;224m\u' \
    '\e[38;2;15;255;240m\u' \
    '\e[38;2;0;255;255m\u' \
    '\e[38;2;16;239;255m\u' \
    '\e[38;2;32;223;255m\u' \
    '\e[38;2;48;207;255m\u' \
    '\e[38;2;64;191;255m\u' \
    '\e[38;2;80;175;255m\u' \
    '\e[38;2;96;159;255m\u' \
    '\e[38;2;112;143;255m\u' \
    '\e[38;2;128;127;255m\u' \
    '\e[38;2;144;111;255m\u' \
    '\e[38;2;160;95;255m\u' \
    '\e[38;2;176;79;255m\u' \
    '\e[38;2;192;63;255m\u' \
    '\e[38;2;208;47;255m\u' \
    '\e[38;2;224;31;255m\u' \
    '\e[38;2;240;15;255m\u' \
    '\e[38;2;255;0;255m\u' \
    '\e[38;2;255;0;239m\u' \
    '\e[38;2;255;0;223m\u' \
    '\e[38;2;255;0;207m\u' \
    '\e[38;2;255;0;191m\u' \
    '\e[38;2;255;0;175m\u' \
    '\e[38;2;255;0;159m\u' \
    '\e[38;2;255;0;143m\u' \
    '\e[38;2;255;0;127m\u' \
    '\e[38;2;255;0;111m\u' \
    '\e[38;2;255;0;95m\u' \
    '\e[38;2;255;0;79m\u' \
    '\e[38;2;255;0;63m\u' \
    '\e[38;2;255;0;47m\u' \
    '\e[38;2;255;0;31m\u' \
    '\e[38;2;255;0;15m\u' 
