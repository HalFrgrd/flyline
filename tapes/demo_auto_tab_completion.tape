Output demo_auto_tab_completion.gif
Output demo_auto_tab_completion.svg
Output demo_auto_tab_completion.json

Source demo_settings.tape
Set Height 600
Set Width 1800
Source demo_setup.tape

Hide
Type@5ms " flyline suggestions --auto-suggest true && clear"
Enter
Show

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
Sleep 900ms
