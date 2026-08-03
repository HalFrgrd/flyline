# Flyline widgets:
flyline create-prompt-widget mouse-mode --name MOUSE_MODE "on " "off"
flyline create-prompt-widget copy-buffer '[copy]'
flyline create-prompt-widget last-command-duration

# Starship widgets:
# NB: for these to work, they must be enabled in starship.toml config
flyline create-prompt-widget custom --name GIT_METRICS --command "starship module git_metrics" --placeholder prev

