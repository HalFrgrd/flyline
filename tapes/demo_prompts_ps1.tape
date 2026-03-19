# Where should we write the GIF?
Output demo_prompts_ps1.gif

Source demo_settings.tape

Set Height 400

Source demo_setup.tape

Type@100ms "export PS1='\u@\h:\w$ '"
Enter
Sleep 1500ms

Type "# let's make it multiline"
Enter
Sleep 1500ms

Type@100ms "export PS1='\u@\h:\w\n$ '"
Enter
Sleep 1500ms

Type "# and let's add some colour using ANSI escape codes"
Enter
Sleep 1500ms

Type@100ms "export PS1='\e[01;32m\u@\h\e[00m:\e[01;34m\w\e[00m\n$ '"
Enter
Sleep 1500ms

Sleep 1500ms
