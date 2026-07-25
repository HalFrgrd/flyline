# Where should we write the GIF?
Output demo_prompts_ps2.gif
Output demo_prompts_ps2.svg

Source demo_settings.tape
Set Height 400
Source demo_setup.tape

Hide
Type@40ms " flyline editor --auto-close-chars false"
Enter
Type@40ms " PS1='\e[01;32m\u@\h\e[00m:\e[01;34m\w\e[00m$ '"
Enter
Type@40ms " RPS1=''"
Enter
Type@40ms " clear"
Enter
Show

Type "# customize multi-line continuation prompt with PS2"
Enter
Sleep 1000ms


Type `PS2='\e[34mline FLYLINE_PROMPT_LINE_NUMBER:\e[0m '`
Enter
Sleep 1000ms

Type `echo "First line`
Enter
Type `Second line`
Enter
Type `Third line"`
Enter
Sleep 2000ms
