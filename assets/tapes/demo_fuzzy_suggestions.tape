# Where should we write the GIF?
Output demo_fuzzy_suggestions.gif

Source demo_settings.tape

Type "grep --"
Sleep 500ms
Tab
Sleep 1000ms
Type "with"
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
Type "wtih"
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

Sleep 2s
