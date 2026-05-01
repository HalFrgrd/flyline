flyline create-prompt-widget custom --name GIT_METRICS --command "starship module git_metrics" --placeholder prev

flyline create-prompt-widget mouse-mode --name MOUSE_MODE "on " "off"
flyline create-prompt-widget copy-buffer '[copy]'
flyline create-prompt-widget last-command-duration --format "%H:%M:%S"
flyline create-prompt-widget last-command-duration --format five-chars
