Output demo_overview.gif
Output demo_overview.svg

Source demo_settings.tape
Set Width 1330
Source demo_setup.tape

Require claude

Hide
Sleep 50ms
Type@5ms `export PATH="/home/john/bin/:$PATH" && flyline suggestions --auto-suggest true && flyline suggestions --num-suggestion-rows 10  && clear`
Enter
Show

#############################################################################################################
# intellisense
#############################################################################################################
Hide
Type clear
Enter
Type@5ms `clear && echo "flyline is a Bash plugin that brings many improvements. For instance, Intellisense style suggestions:" && echo ""`
Enter
Sleep 100ms
Show

Sleep 2000ms
Type "gre"
Sleep 1000ms
Tab
Enter
Sleep 600ms
Type "--"
Sleep 800ms
Type `matches`
Sleep 800ms
Tab
Sleep 500ms
Enter
Sleep 500ms
Type `something `
Sleep 500ms

Type `~/`
Sleep 800ms
Tab
Sleep 800ms
Enter
Sleep 800ms

Hide
Ctrl+C
Type@5ms `flyline suggestions --auto-suggest false && clear`
Enter
Show


# ############################################################################################################
# # Prompt navigation
# ############################################################################################################

Hide
Sleep 100ms
Ctrl+C
Type@5ms `cd ~/foo/bar/baz`
Enter
Sleep 100ms
Type@5ms `clear && echo "Navigate using your prompt:" && echo ""`
Enter
Show

Sleep 800ms
Left
Sleep 600ms
Left
Sleep 600ms
Left
Sleep 600ms
Left
Sleep 600ms
Enter
Sleep 1000ms

Hide
Type@5ms `cd ~/`
Enter
Show


# #############################################################################################################
# # fuzzy history search
# #############################################################################################################

Hide
Sleep 100ms
Ctrl+C
Type@5ms `clear`
Enter
Type `# Fuzzy history search`
Show

Sleep 1500ms
Ctrl+Backspace 4

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
# # AI agent integration
# ############################################################################################################
Hide
Sleep 100ms
Ctrl+C
Type@5ms `clear && echo "flyline can interface with your AI agent to help you write commands:" && echo ""`
Enter
Show

Sleep 800ms
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

#############################################################################################################
# tooltips
#############################################################################################################

Hide
Sleep 100ms
Ctrl+C
Type@5ms `clear && echo "flyline's deep Bash integration allows tooltips as you type:" && echo ""`
Enter
Show

Sleep 800ms
Type `ls`
Sleep 1500ms
Type ` $HOME`
Sleep 1500ms
Type ` && echo $BASH_VERSINFO`
Sleep 2000ms


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
Backspace 3
Sleep 800ms
Backspace@800ms 3
Sleep 800ms


Hide
Ctrl+C
Type@5ms `flyline editor --auto-close-chars false `
Enter
Show

############################################################################################################
# Text selection
############################################################################################################


Hide
Ctrl+C
Type@5ms `clear && echo "flyline supports code editor-like text selection" && echo "This has nothing to do with the terminal emulator, it's all inside of Bash!" && echo ""`
Enter
Show

Sleep 800ms
Type `git commit -m a multi word message`
Sleep 1000ms
Ctrl+Shift+Left              # making use of evp's support for all types of keybindings!
Sleep 500ms
Ctrl+Shift+Left
Sleep 500ms
Ctrl+Shift+Left
Sleep 500ms
Ctrl+Shift+Left
Sleep 1000ms
Type '"'
Sleep 600ms
Right
Right
Sleep 800ms

# ############################################################################################################
# # conclusion
# ############################################################################################################


Hide
Ctrl+C
Type@5ms `clear && echo "Check out the rest of this README to learn about flyline's other features! Like dynamic prompts:" && echo ""`
Enter
Show

Sleep 800ms
Type `RPS1='\t'`
Enter

Sleep 4s
