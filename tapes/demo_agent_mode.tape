# Where should we write the GIF?
Output demo_agent_mode.gif

Source demo_settings.tape
Set Height 1200
Source demo_setup.tape

Type `flyline agent-mode \`
Enter
Type `  --system-prompt "Be concise. Answer with a JSON array of <=3 items with objects containing command and description. Command will be a bash command." \`
Enter
Type "  --command claude --prompt"
Enter
Sleep 500ms

Type "list files older than three days"
Sleep 500ms
Shift+Enter
Sleep 5000ms
