eval "$(starship init bash)"

alias ll="ls -alF"
export HISTTIMEFORMAT="%F %T  "
history -a

enable -f ./target/debug/libjobu.so jobu
