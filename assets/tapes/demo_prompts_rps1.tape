# Where should we write the GIF?
Output demo_prompts_rps1.gif

Type "export PS1='\[\033[01;32m\]\u@\h\[\033[00m\]:\[\033[01;34m\]\w\[\033[00m\]\n$ '"
Enter

Source demo_settings.tape
Set Height 200
Source demo_setup.tape

Type `export RPS1="I'm on the right"`
Enter
Sleep 1500ms

Type `# let's set the right prompt to show the time`
Enter
Sleep 1500ms

Type "export RPS1='\t'"
Enter
Sleep 1500ms

Type `# RPS1 can span multiple lines`
Enter
Sleep 1500ms

Type "export RPS1='\t\n<'"
Enter
Sleep 1500ms

Sleep 1500ms
