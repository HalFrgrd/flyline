# Where should we write the GIF?
Output demo_prompts_ps1_fill.gif


Source demo_settings.tape
Set Height 400
Source demo_setup.tape

Hide
Type@1ms "export PS1='\e[01;32m\u@\h\e[00m:\e[01;34m\w\e[00m\n$ '"
Enter
Type@1ms "export RPS1=' \e[01;33m\t\n<\e[00m'"
Enter
Type@1ms "clear"
Enter
Show


Type "# let's fill in the gap by setting PS1_FILL"
Enter
Sleep 1000ms

Type "export PS1_FILL='-'"
Enter
Sleep 1500ms


Type "# PS1_FILL can be multiple characters"
Enter
Sleep 1000ms

Type "export PS1_FILL='🯁🯂🯃🮲🮳'"
Enter
Sleep 1000ms

Type "# If you so desire, PS1_FILL can support time formats"
Enter
Sleep 1000ms

Type "export PS1_FILL='🯁🯂🯃🮲🮳 \D{%S}'"
Enter
Sleep 1000ms

Type "# wonderful. look at our beautiful prompt"
Sleep 3s
