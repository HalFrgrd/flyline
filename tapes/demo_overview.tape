# Where should we write the GIF?
Output demo_overview.gif
Output demo_overview.svg

Source demo_settings.tape
Source demo_setup.tape

Require claude

Hide
Sleep 50ms
Type@5ms `export PATH="/home/john/bin/:$PATH" && flyline suggestions --auto-suggest true && flyline suggestions --num-suggestion-rows 8  && flyline editor --auto-close-chars true && flyline key bind PageUp always=moveLeftOneWordExtendSelection && clear`
Enter
Show

Hide
Type clear
Enter
Type@5ms "# flyline is a full terminal app that replaces Bash's line editing functionality."
Enter
Type@5ms "# There are many improvements like Intellisense style suggestions:"
Enter
Sleep 100ms
Show

Sleep 1000ms
Type "gre"
Sleep 600ms
Tab
Enter
Sleep 900ms
Type "--"
Sleep 600ms
Type `matches`
Sleep 600ms
Tab
Sleep 600ms
Enter
Type `foo `
Sleep 600ms

Type `~/`
Sleep 900ms
Tab
Tab
Sleep 900ms
Ctrl+C

Hide
Type@5ms `flyline suggestions --auto-suggest false && clear `
Enter
Sleep 100ms
Type@5ms `# flyline integrates deeply with Bash allowing tooltips as you type:`
Enter
Show

Sleep 800ms
Type `ls`
Sleep 1500ms
Type ` $HOME`
Sleep 1500ms
Type ` && echo $BASH_VERSINFO`
Sleep 2000ms
Ctrl+C
Sleep 800ms

Hide
Type `clear`
Enter
Type `# fuzzy history search:`
Enter
Show
Sleep 800ms
Ctrl+R
Sleep 1500ms
Type `crgo`
Sleep 800ms
Down
Sleep 500ms
Down
Sleep 500ms
Up
Sleep 500ms
Up
Sleep 500ms
Enter
Sleep 500ms
Ctrl+C
Sleep 800ms

Hide
Type `cd ~/foo/bar/baz && clear`
Enter
Sleep 100ms
Type "# navigation using your prompt:"
Enter
Show

Sleep 800ms
Left
Sleep 500ms
Left
Sleep 500ms
Left
Sleep 500ms
Left
Sleep 500ms
Enter
Sleep 800ms

# Hide
# Type `# flyline can interface with your AI agent to help you write commands`
# Enter
# Sleep 800ms
# Show

# Type "list files older than three days"
# Sleep 500ms
# Alt+Enter
# Sleep 4000ms
# Down
# Sleep 1000ms
# Down
# Sleep 1000ms
# Up
# Sleep 1000ms
# Enter
# Sleep 800ms
# Enter


# Hide
# Type `# flyline can intelligently close opening quotes and brackets: `
# Enter
# Show

# Sleep 800ms
# Type `echo "`
# Sleep 1000ms
# Type `$(`
# Sleep 1000ms
# Type `foo`
# Sleep 800ms
# Backspace
# Backspace
# Backspace
# Sleep 800ms
# Backspace
# Sleep 1000ms
# Backspace
# Sleep 1000ms
# Backspace
# Sleep 800ms
# Ctrl+C
# Sleep 800ms

# Hide
# Type `# flyline also supports text selection like you would expect in a code editor`
# Enter
# Show

# Sleep 800ms
# Type `git commit -m a multi word message`
# Sleep 1000ms
# # I rigged page up to select left one word
# PageUp
# Sleep 500ms
# PageUp
# Sleep 500ms
# PageUp
# Sleep 500ms
# PageUp
# Sleep 1000ms
# Type `"`
# Sleep 1500ms
# Escape
# Sleep 500ms
# Ctrl+C
# Sleep 800ms

# Type `# Check out the rest of this README to learn about flyline's other features! Like dynamic prompts:`
# Enter
# Type `RPS1='\t`
# Enter

# Sleep 4s
