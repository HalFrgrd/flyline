Output demo_overview.gif
Output demo_overview.svg

Source demo_settings.tape
Source demo_setup.tape

Require claude

Hide
Sleep 50ms
Type@5ms `export PATH="/home/john/bin/:$PATH" && flyline suggestions --auto-suggest true && flyline suggestions --num-suggestion-rows 8  && clear`
Enter
Show

#############################################################################################################
# intellisense
#############################################################################################################
Hide
Type clear
Enter
Type@5ms `clear && echo "flyline is a full terminal app that replaces Bash's line editing functionality." && echo "There are many improvements like Intellisense style suggestions:" && echo ""`
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

#############################################################################################################
# tooltips
#############################################################################################################

Hide
Ctrl+C
Type@5ms `flyline suggestions --auto-suggest false `
Enter
Sleep 100ms
Type@5ms `clear && echo "flyline integrates deeply with Bash allowing tooltips as you type:" && echo ""`
Enter
Show

Sleep 800ms
Type `ls`
Sleep 1500ms
Type ` $HOME`
Sleep 1500ms
Type ` && echo $BASH_VERSINFO`
Sleep 2000ms

# #############################################################################################################
# # fuzzy history search
# #############################################################################################################

Hide
Ctrl+C
Type@5ms `clear && echo "fuzzy history search:" && echo ""`
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
Sleep 800ms

# ############################################################################################################
# # Prompt navigation
# ############################################################################################################

Hide
Ctrl+C
Type@5ms `cd ~/foo/bar/baz`
Enter
Sleep 100ms
Type@5ms `clear && echo "navigation using your prompt:" && echo ""`
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

Hide
Type@5ms `cd ~/`
Enter
Show

# ############################################################################################################
# # AI agent integration
# ############################################################################################################
Hide
Ctrl+C
Type@5ms `clear && echo "flyline can interface with your AI agent to help you write commands" && echo ""`
Enter
Sleep 800ms
Show

Type "list files older than three days"
Sleep 500ms
Alt+Enter
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



# ############################################################################################################
# # Intelligent quote and bracket closing
# ############################################################################################################

Hide
Ctrl+C
Type@5ms `flyline editor --auto-close-chars true `
Enter
Type@5ms `clear && echo "flyline can intelligently close opening quotes and brackets:" && echo ""`
Enter
Show

Sleep 800ms
Type 'echo "'
Sleep 1000ms
Type `$(`
Sleep 1000ms
Type `foo`
Sleep 800ms
Backspace
Backspace
Backspace
Sleep 800ms
Backspace
Sleep 1000ms
Backspace
Sleep 1000ms
Backspace
Sleep 800ms
Ctrl+C
Sleep 800ms

Hide
Type@5ms `flyline editor --auto-close-chars false `
Enter
Show

# ############################################################################################################
# # Text selection
# ############################################################################################################


Hide
Ctrl+C
Type@5ms `clear && echo "flyline also supports text selection like you would expect in a code editor" && echo ""`
Enter
Show

Sleep 800ms
Type `git commit -m a multi word message`
Sleep 1000ms
# I rigged page up to select left one word
Ctrl+Shift+Left
Sleep 500ms
Ctrl+Shift+Left
Sleep 500ms
Ctrl+Shift+Left
Sleep 500ms
Ctrl+Shift+Left
Sleep 1000ms
Type '"'
Sleep 1500ms
Escape
Sleep 500ms
Ctrl+C
Sleep 800ms

# ############################################################################################################
# # conclusion
# ############################################################################################################


Hide
Ctrl+C
Type@5ms `clear && echo "Check out the rest of this README to learn about flyline's other features! Like dynamic prompts:" && echo ""`
Enter
Show

Type `RPS1='\t'`
Enter

Sleep 4s
