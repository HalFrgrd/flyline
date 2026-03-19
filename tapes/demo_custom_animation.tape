# Where should we write the GIF?
Output demo_custom_animation.gif

Source demo_settings.tape
Set Height 400
Source demo_setup.tape

Type "flyline create-anim --name MY_ANIMATION --fps 10 ⣾ ⣷ ⣯ ⣟ ⣻ ⣽"
Enter
Sleep 500ms

Type "export PS1='MY_ANIMATION \u@\h:\w$ '"
Enter
Sleep 3000ms
