# Where should we write the GIF?
Output demo_custom_animation.gif

Source demo_settings.tape
Set Height 400
Source demo_setup.tape

Hide
Type `export PS1="${PS1::-2} " && clear`
Enter
Show


Type "flyline create-anim --name MY_ANIMATION --fps 10 ⣾ ⣷ ⣯ ⣟ ⡿ ⢿ ⣻ ⣽"
Enter
Sleep 500ms

Type `export PS1="${PS1}MY_ANIMATION "`
Enter
Sleep 1500ms

Type `# any text in the prompt is considered for replacement so you can animate usernames, paths, branch names etc...`
Enter
Sleep 1000ms

Type `flyline create-anim --name "john" --ping-pong --fps 8 john john john John jOhn joHn johN john john`
Enter

Sleep 3000ms
