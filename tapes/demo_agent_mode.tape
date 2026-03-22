# Where should we write the GIF?
Output demo_agent_mode.gif

Source demo_settings.tape
Source demo_setup.tape

Require claude

Type `flyline agent-mode --system-prompt "Be concise. Answer with a JSON array of at most 3 items with objects containing 'command' and 'description'. Command will be a bash command." \`
Enter
Type `  --command claude --prompt --effort low`
Enter
Sleep 500ms

Type "list files older than three days"
Sleep 500ms
# special shortcut for agent mode
Alt+I
Sleep 4000ms
Down
Sleep 1000ms
Down
Sleep 1000ms
Up
Sleep 1000ms
Enter

