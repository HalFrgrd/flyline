eval "$(starship init bash)"

alias ll="ls -alF"

enable -f ./target/debug/libjobu.so jobu
jobu -s # TODO: make this happen on load automatically
