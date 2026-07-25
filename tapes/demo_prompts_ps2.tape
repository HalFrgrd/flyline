# Where should we write the GIF?
Output demo_prompts_ps2.gif
Output demo_prompts_ps2.svg

Source demo_settings.tape
Set Height 400
Source demo_setup.tape

Hide
Type@40ms " flyline editor --auto-close-chars false"
Enter
Type@40ms " clear"
Enter
Show

Type "# customize multi-line continuation prompt with PS2"
Enter
Sleep 1000ms

Type `PS2='\e[2mline FLYLINE_PROMPT_LINE_NUMBER:\e[0m '`
Enter
Sleep 1000ms

Type `echo "First line`
Enter
Sleep 500ms
Type `Second line`
Enter
Sleep 500ms
Type `Third line"`
Enter
Sleep 500ms
Sleep 2000ms
