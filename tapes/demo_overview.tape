# Where should we write the GIF?
Output demo_overview.gif

Source demo_settings.tape
Source demo_setup.tape

Require claude


Hide
Type `flyline --auto-close-chars true && clear`
Enter
Show

Type "# bash is great but the command line editing user  experience feels outdated."
Enter
Sleep 800ms

Type "# flyline is full terminal app that replaces bash's line editing functionality."
Enter
Sleep 800ms

Type "# flyline can draw anywhere on the terminal meaning we can pretty much anything:"
Enter
Sleep 800ms

Type `flyline --matrix-animation`
Enter
Sleep 3000ms

Type `flyline --matrix-animation false`
Enter
Sleep 500ms

Type "# flyline integrates deeply with bash allowing it  to extend bash's tab completions, history searching, and more."
Enter
Sleep 800ms

Type `# How about tooltips as you type:`
Enter
Type `ls`
Sleep 1500ms
Type ` $HOME`
Sleep 1500ms
Type `/`
Enter
Sleep 800ms

Type `# Or fuzzy history searching like fzf:`
Enter
Ctrl+R
Down
Sleep 500ms
Down
Sleep 500ms
Down
Sleep 500ms
Down
Sleep 500ms
Down
Sleep 500ms
Down
Sleep 500ms
Down
Sleep 500ms
Down
Sleep 500ms
Down
Sleep 500ms
Enter
Sleep 800ms


Type `# Flyline can intelligently close opening quotes and brackets: `
Enter
Sleep 800ms
Type `echo "`
Sleep 300ms
Type `$(`
Sleep 300ms
Type `foo`
Backspace
Backspace
Backspace
Sleep 300ms
Backspace
Sleep 300ms
Backspace
Sleep 300ms
Backspace
Ctrl+C



# TODO: show agent mode


Sleep 800ms
Type `python3 -c 'print("like animating the command buffer")'`
Sleep 1500ms
Enter

Type `# How about dynamic prompts:`
Enter
Type `export RPS1="\t"`
Enter



Type `# Check out the rest of this README to learn about flyline's other features`

Sleep 5s
