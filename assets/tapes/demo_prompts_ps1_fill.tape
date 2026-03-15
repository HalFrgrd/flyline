# Where should we write the GIF?
Output demo_prompts_ps1_fill.gif

Type "export PS1='\[\033[01;32m\]\u@\h\[\033[00m\]:\[\033[01;34m\]\w\[\033[00m\]\n$ '"
Type "export RPS1='\t\n<'"

Source demo_settings.tape

Set Height 200
Source demo_setup.tape


Type "# let's fill in the gap by setting PS1_FILL"
Enter
Sleep 1000ms

Type "export PS1_FILL='-'"
Enter
Sleep 1500ms


Type "# PS1_FILL can be multiple chars"
Enter
Sleep 1000ms

Type "export PS1_FILL='🯁🯂🯃🮲🮳'"
Enter
Sleep 1000ms

Type "# PS1_FILL can also support time formats"
Enter
Sleep 1000ms

Type "export PS1_FILL='🯁🯂🯃🮲🮳 \D{%.3f}'"
Enter
Sleep 1000ms

Type "# wonderful. look at our beautiful prompt"
Sleep 3s
