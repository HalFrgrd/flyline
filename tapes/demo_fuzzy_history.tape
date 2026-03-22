# Where should we write the GIF?
Output demo_fuzzy_history.gif

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
Alt+I
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
Alt+I
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
Sleep 500ms
Enter
Sleep 2000ms
Ctrl+R
Sleep 2000ms
Down
Sleep 1000ms
Up
Sleep 2000ms


Sleep 1000ms
