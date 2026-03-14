# Where should we write the GIF?
Output demo_fuzzy_suggestions.gif

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

Type "# demo of fuzzy completions"
Enter
Sleep 500ms

Type "grep --"
Sleep 500ms
Tab
Sleep 1000ms
Type "file"
Sleep 1000ms
Down
Sleep 500ms
Down
Sleep 500ms
Enter
Sleep 500ms
Ctrl+C
Sleep 1000ms

Type "# you can make spelling mistakes"
Enter
Sleep 500ms
Type "grep --"
Sleep 500ms
Tab
Sleep 1000ms
Type "fiel"
Sleep 1000ms
Down
Sleep 500ms
Down
Sleep 500ms
Enter
Sleep 500ms
Ctrl+C
Sleep 1000ms

Type "# the left hand side is used for programmable completions but the entire word is used for fuzzy searching"
Enter
Sleep 500ms
Type "grep --filemtch"
Sleep 500ms
Left
Left
Left
Left
Sleep 500ms
Tab
Sleep 1000ms
Down
Sleep 500ms
Down
Sleep 500ms
Enter

# Admire the output for a bit.
Sleep 2s
