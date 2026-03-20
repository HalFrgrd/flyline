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
