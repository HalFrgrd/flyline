# Where should we write the GIF?
Output demo_agent_mode.gif

Source demo_settings.tape
Set Height 800
Source demo_setup.tape

Type `flyline agent-mode \`
Enter
Type `  --system-prompt "Be concise. Answer with a JSON array of <=3 items with objects containing command and description. Command will be a bash command." \`
Enter
Type "  --command claude --prompt --effort low"
Enter
Sleep 500ms

Type "list files older than three days"
Sleep 500ms
Escape
Type@1ms `[13;2u`
Sleep 3500ms
Down
Sleep 1000ms
Down
Sleep 1000ms
Up
Sleep 1000ms
Enter
Enter
Sleep 500ms
Type "cd ~ && flyline --dump-logs flyline_agent_mode.log"
Enter