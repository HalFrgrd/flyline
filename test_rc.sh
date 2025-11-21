eval "$(starship init bash)"

enable -f ./target/debug/libjobu.so jobu
# can't set bash_input right now since bash_input is being used to read this file
jobu -s
# echo "asdfsdfsdf"
# PROMPT_COMMAND="$PROMPT_COMMAND; jobu -r"
