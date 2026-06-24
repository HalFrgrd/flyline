# Where should we write the GIF?
Output demo_prompts_final.gif
Output demo_prompts_final.svg

Source demo_settings.tape
Set Height 400
Source demo_setup.tape

Hide
Type@1ms " PS1='\e[01;32m\u@\h\e[00m:\e[01;34m\w\e[00m\n$ '"
Enter
Type@1ms " RPS1=' \e[01;33m\t\n<\e[00m'"
Enter
Type@1ms " clear"
Enter
Show

Type "# Let's configure a transient prompt"
Enter
Sleep 1000ms

Type `PS1_FINAL='Started at \D{%Y-%m-%d %H:%M:%S} > ' && RPS1_FINAL='' && PS1_FILL=''`
Enter
Sleep 1500ms

Type "touch foo"
Sleep 1500ms
Enter

Sleep 2000ms
Type "# See how the submitted prompt shrank to the date and time!"
Sleep 1000ms
Enter

Sleep 1000ms
Type "touch bar"
Sleep 1000ms
Enter

Sleep 1000ms
Type "touch baz"
Sleep 1000ms
Enter

Sleep 3000ms
