# Where should we write the GIF?
Output demo_prompts.gif

# Set up a 1560x780 terminal with 31px font (30% bigger than original 1200x600 / 24px).
Set FontSize 31
Set Width 1560
Set Height 780
Set Framerate 60

Hide

# vhs doesnt source our bashrc so lets run bash here so that it will source it
Type "clear && bash"
Enter

Show

Type "# Let's set some prompts!"
Enter
Sleep 1000ms

Type "export PS1='\u@\h:\w$ '"
Enter
Sleep 1500ms

Type "# now let's set the right hand side prompt to show the time"
Enter
Sleep 1000ms

Type "export RPS1='\t'"
Enter
Sleep 1500ms

Type "# how about a multiline prompt:"
Enter
Sleep 1000ms

Type "PS1='\u@\h:\w\n$ '"
Enter
Sleep 1500ms

Type "# and let's fill in the gap"
Enter
Sleep 1000ms

Type "export PS1_FILL='-'"
Enter
Sleep 1500ms

Type "# let's add some colour"
Enter
Sleep 1000ms

Type "PS1='\[\033[01;32m\]\u@\h\[\033[00m\]:\[\033[01;34m\]\w\[\033[00m\]\n$ '"
Enter
Sleep 1500ms

Type "# PS1_FILL can be multiple chars"
Enter
Sleep 1000ms

Type "export PS1_FILL='▁▂▃▄▅▆▇█▇▆▅▄▃▂▁'"
Enter

Type "# wonderful. look at our beautiful prompt"
# Admire the output for a bit.
Sleep 2s
