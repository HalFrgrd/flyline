Output demo_glob_expansion.gif
Output demo_glob_expansion.svg

Source demo_settings.tape
Set Height 600
Set Width 1400
Source demo_setup.tape

Hide
Type@40ms " flyline suggestions --auto-suggest true && shopt -s extglob && cd /home/john/project && clear"
Enter
Show

Type "ls @(Cargo.*|README*)"
Sleep 1200ms
Enter
Sleep 1000ms

Type "ls !(*.rs)"
Sleep 1200ms
Enter
Sleep 1000ms

Type "ls src/@(main|lib).rs"
Sleep 1200ms
Enter
Sleep 1500ms
