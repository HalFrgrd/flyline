# Where should we write the GIF?
Output demo_fuzzy_suggestions.gif

Source demo_settings.tape
Source demo_setup.tape

Type "caro"
Sleep 500ms
Ctrl+R
Sleep 1000ms
Down
Sleep 500ms
Down
Sleep 500ms
Enter
Sleep 500ms
Ctrl+C
Sleep 1000ms

Type "# Flyline uses skim-rs' `arinae` typo-resistant algorithm:"
Enter
Sleep 500ms
Type "cagro"
Sleep 500ms
Ctrl+R
Sleep 1000ms
Down
Sleep 500ms
Down
Sleep 500ms
Enter
Sleep 500ms
Ctrl+C
Sleep 1000ms


Type "# Multiline commands are expanded when selected:"
Enter
Sleep 500ms
Type `echo "abc`
Enter
Type `def`
Enter
Type `ghi`
Enter
Type `jkl"`
Enter
Sleep 1000ms
Ctrl+R
Sleep 1000ms
Down
Sleep 500ms
Up
Sleep 1000ms



