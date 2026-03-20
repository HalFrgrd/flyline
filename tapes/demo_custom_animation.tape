# Where should we write the GIF?
Output demo_custom_animation.gif

Source demo_settings.tape
Set Height 400
Source demo_setup.tape

Type "flyline create-anim --name MY_ANIMATION --fps 10 ⣾ ⣷ ⣯ ⣟ ⣻ ⣽"
Enter
Sleep 500ms

Type `export PS1="${PS1::-2}MY_ANIMATION "`
Enter
Sleep 1500ms

Type `flyline create-anim --name "john" --ping-pong --fps 8 \`
Enter
Type  "john \"
Enter
Type  "john \"
Enter
Type  "john \"
Enter
Type  "John \"
Enter
Type  "jOhn \"
Enter
Type  "joHn \"
Enter
Type  "johN \"
Enter
Type  "john \"
Enter
Type  "john"
Enter

Sleep 3000ms
