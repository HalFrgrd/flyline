# Where should we write the GIF?
Output demo_prompts_ps1_fill.gif
Output demo_prompts_ps1_fill.svg

Source demo_settings.tape
Set Height 400
Source demo_setup.tape

Type "# let's fill in the gap by setting PS1_FILL"
Enter
Sleep 1000ms

Type "PS1_FILL='-'"
Enter
Sleep 1500ms

Type "# PS1_FILL can be multiple characters"
Enter
Sleep 1000ms

Type "PS1_FILL='🯁🯂🯃🮲🮳'"
Enter
Sleep 1000ms

Type "# If you so desire, PS1_FILL can support time formats"
Enter
Sleep 1000ms

Type "PS1_FILL='🯁🯂🯃🮲🮳 \D{%S}'"
Enter
Sleep 1000ms

Type "# Wonderful. Look at our beautiful prompt!"
Sleep 3s
