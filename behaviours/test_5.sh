
# strace -s  99 -tt -e read,write  bash --init-file behaviours/test_5.sh

# 22:22:03.561568 read(0, "a", 1)         = 1
# 22:22:03.561641 write(2, "H", 1H)        = 1
# 22:22:03.561686 write(2, "e", 1e)        = 1
# 22:22:03.561728 write(2, "l", 1l)        = 1
# 22:22:03.561775 write(2, "l", 1l)        = 1
# 22:22:03.561822 write(2, "o", 1o)        = 1

# 22:22:22.745376 read(0, "b", 1)         = 1
# ) = 5
# 22:22:23.747812 --- SIGCHLD {si_signo=SIGCHLD, si_code=CLD_EXITED, si_pid=100286, si_uid=1000, si_status=0, si_utime=0, si_stime=0} ---
# 22:22:23.748334 write(2, "myprompt> ", 10myprompt> ) = 10
# 22:22:23.748385 write(2, "H", 1H)        = 1
# 22:22:23.748419 write(2, "e", 1e)        = 1
# 22:22:23.748452 write(2, "l", 1l)        = 1
# 22:22:23.748484 write(2, "l", 1l)        = 1
# 22:22:23.748517 write(2, "o", 1o)        = 1

# notice macros are echoed to the terminal
# but if a bind -x is used, the prompt is first and then command is echoed

PS1="myprompt> "


bind '"a": "Hello"'

bind '"b": "zHello"'
bind -x '"z": sleep 1'
