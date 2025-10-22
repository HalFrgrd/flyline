# strace -s  99 -tt -e read,write  bash --init-file behaviours/test_6.sh
# all of these give the bell

bind '"a": "\e[31mHello\e[0m"'
bind '"b": "\001\e[31m\002Hello\001\e[0m\002"'
bind '"c": "\033[31mHello\033[0m"'
bind '"d": "\001\033[31m\002Hello\001\033[0m\002"'
bind '"z": "\x01\033[1;34m\x02>\x01\033[0m\x02"'