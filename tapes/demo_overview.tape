# Where should we write the GIF?
Output demo_overview.gif

Source demo_settings.tape
Source demo_setup.tape

Require claude

Hide
Type `flyline --auto-close-chars true && clear`
Enter
Show

Type "# flyline is a full terminal app that replaces bash's line editing functionality."
Enter
Sleep 800ms

Type "# flyline can draw anywhere on the terminal meaning we can do pretty much anything:"
Enter
Sleep 800ms

Type `flyline --matrix-animation`
Enter
Sleep 3000ms

Type `flyline --matrix-animation false`
Enter
Sleep 500ms

Type `# flyline integrates deeply with bash allowing it to extend bash's tab completions, history searching, and more.`
Enter
Sleep 800ms

Type `# How about tooltips as you type:`
Enter
Sleep 800ms
Type `ls`
Sleep 1500ms
Type ` $HOME`
Sleep 1500ms
Type `/`
Enter
Sleep 800ms

Type `# Or fuzzy history searching like fzf:`
Enter
Sleep 800ms
Type `hs`
Sleep 800ms
Ctrl+R
Down
Sleep 500ms
Down
Sleep 500ms
Up
Sleep 500ms
Up
Sleep 500ms
Enter
Sleep 800ms

Type `# Flyline can intelligently close opening quotes and brackets: `
Enter
Sleep 800ms
Type `echo "`
Sleep 1000ms
Type `$(`
Sleep 1000ms
Type `foo`
Sleep 800ms
Backspace
Backspace
Backspace
Sleep 300ms
Backspace
Sleep 1000ms
Backspace
Sleep 1000ms
Backspace
Ctrl+C
Sleep 800ms

Type `# Flyline can call your command line agent to help you write commands`
Enter
Sleep 800ms
Type "list files older than three days"
Sleep 500ms
# special shortcut for agent mode
Alt+I
Sleep 4000ms
Down
Sleep 1000ms
Down
Sleep 1000ms
Up
Sleep 1000ms
Enter
Sleep 800ms
Enter

Type `# Check out the rest of this README to learn about flyline's other features! Like dynamic prompts:`
Enter
Type `export RPS1='\t`
Enter

Sleep 5s
