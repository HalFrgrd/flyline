# Flyline widgets:
flyline create-prompt-widget mouse-mode --name MOUSE_MODE "on " "off"
flyline create-prompt-widget copy-buffer '[copy]'
flyline create-prompt-widget last-command-duration

# Starship widgets:
# NB: for these to work, they must be enabled in starship.toml config
flyline create-prompt-widget custom --name GIT_METRICS --command "starship module git_metrics" --placeholder prev

# Flyline performs some simple expansion on the command args so you can use env vars as args:
export RPS1=' STARSHIP_CMD_DURATION'
flyline create-prompt-widget custom  --name STARSHIP_CMD_DURATION --command '/home/hal/.cargo/bin/starship module cmd_duration --cmd-duration=$STARSHIP_DURATION' --placeholder prev --block
