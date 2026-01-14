eval "$(starship init bash)"

alias ll="ls -alF"
export HISTTIMEFORMAT="%F %T  "

enable -f ./target/debug/libflyline.so flyline
