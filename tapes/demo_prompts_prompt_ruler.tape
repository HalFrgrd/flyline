# Where should we write the GIF?
Output demo_prompts_prompt_ruler.gif
Output demo_prompts_prompt_ruler.svg

Source demo_settings.tape
Set Height 400
Source demo_setup.tape

Type "# Separate commands with a horizontal ruler"
Enter
Sleep 1000ms

Type "PROMPT_RULER='─'"
Enter
Sleep 1000ms

Type "echo 'Hello world!'"
Enter
Sleep 1500ms

Type "# Style the ruler with ANSI colours"
Enter
Sleep 1000ms

Type "PROMPT_RULER=$'\e[90m─\e[0m'"
Enter
Sleep 1000ms

Type "ls -la"
Enter
Sleep 1500ms

Type "# Or use empty-line for blank line separation"
Enter
Sleep 1000ms

Type "PROMPT_RULER='empty-line'"
Enter
Sleep 1000ms

Type "date"
Enter
Sleep 2500ms
