# Where should we write the GIF?
Output demo_agent_mode.gif

Source demo_settings.tape
Source demo_setup.tape

Require claude

Hide
Sleep 50ms
Type `export PATH="/home/john/bin/:$PATH" && clear`
Enter
Show

Type `flyline agent-mode --system-prompt "Answer with a JSON array of at most 3 items with objects containing: command and description. Command will be a Bash command." \`
Enter
Type `  --trigger-prefix "ai: " \`
Enter
Type `  --command claude --prompt --effort low`
Enter
Sleep 500ms

Type "list files older than three days"
Sleep 500ms
Alt+Enter
Sleep 4000ms
Down
Sleep 1000ms
Down
Sleep 1000ms
Up
Sleep 1000ms
Enter
Sleep 800ms
Enter

Type "# If you set `--trigger-prefix`, then you can launch agent mode simply with Enter:`
Enter
Sleep 500ms
Type "ai: list files older than three days"
Sleep 4000ms
Enter

