# Where should we write the GIF?
Output demo_prompts_rps1.gif

Source demo_settings.tape
Set Height 400
Source demo_setup.tape

Hide
Type@1ms `export PS1='\[\e[01;32m\]\u@\h\[\e[00m\]:\[\e[01;34m\]\w\[\e[00m\]\n$ ' && clear`
Enter
Show

Type `export RPS1="I'm on the right"`
Enter
Sleep 1500ms

Type `# let's set the right prompt to show the time`
Enter
Sleep 1500ms

Type@100ms "export RPS1='\t'"
Enter
Sleep 1500ms

Type `# RPS1 can also span multiple lines`
Enter
Sleep 1500ms

Type@100ms "export RPS1='\t\n<'"
Enter
Sleep 1500ms

Type `# let's make it coloured`
Enter
Sleep 1500ms

Type@100ms "export RPS1='\e[01;33m\t\n<\e[00m'"
Enter
Sleep 1500ms

Sleep 1500ms
