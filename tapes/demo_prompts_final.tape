# Where should we write the GIF?
Output demo_prompts_final.gif
Output demo_prompts_final.svg

Source demo_settings.tape
Set Height 400
Source demo_setup.tape

Type "# Let's configure a transient prompt"
Enter
Sleep 1000ms

Type `PS1_FINAL='Ran at \D{%Y-%m-%d %H:%M:%S}> '`
Enter
Sleep 1500ms

Type "touch foo"
Sleep 1500ms
Enter

Sleep 2000ms
Type "# See how the prompt became the date and time!"
Sleep 1000ms
Enter

Sleep 1000ms
Type "touch bar"
Sleep 1000ms
Enter

Sleep 1000ms
Type "touch baz"
Sleep 1000ms
Enter

Sleep 3000ms
