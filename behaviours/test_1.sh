# run with `bash --init-file behaviours/test_1.sh`
# this one has a good flow
# strace -s 9999 -tt -e read,write bash --init-file behaviours/test_1.sh

# Output from strace with enable-bracketed-paste off:
# 20:51:48.330838 read(0, "s", 1)         = 1
# 20:51:48.439351 read(0, "l", 1)         = 1
# 20:51:48.565256 read(0, "e", 1)         = 1
# 20:51:48.707750 read(0, "e", 1)         = 1
# 20:51:48.814202 read(0, "p", 1)         = 1
# 20:51:48.985572 read(0, " ", 1)         = 1
# 20:51:49.450697 read(0, "1", 1)         = 1
# 20:51:49.856838 read(0, "\r", 1)        = 1
# # then it starts executing the command
# 20:51:49.858738 --- SIGCHLD {si_signo=SIGCHLD, si_code=CLD_EXITED, si_pid=2835059, si_uid=1000, si_status=0, si_utime=0, si_stime=0} ---

# Output from strace with enable-bracketed-paste on:
# 20:52:07.125115 read(0, "s", 1)         = 1
# 20:52:07.225403 read(0, "l", 1)         = 1
# 20:52:07.352084 read(0, "e", 1)         = 1
# 20:52:07.486201 read(0, "e", 1)         = 1
# 20:52:07.595788 read(0, "p", 1)         = 1
# 20:52:07.741153 read(0, " ", 1)         = 1
# 20:52:07.914240 read(0, "1", 1)         = 1
# 20:52:08.178322 read(0, "\r", 1)        = 1
# 20:52:08.178419 write(2, "\33[?2004l\r\n", 10       # wtf!
# ) = 10
# 20:52:08.180364 --- SIGCHLD {si_signo=SIGCHLD, si_code=CLD_EXITED, si_pid=2835661, si_uid=1000, si_status=0, si_utime=0, si_stime=0} ---



# output should be:
# MYPROMPT>              # you press z here
# hellow           
# MYPROMPT>              # you press z here
# hellow
# MYPROMPT>              # you press z here
# hellow

bind "set enable-bracketed-paste off"

PROMPT_COMMAND="stty -echo"
PS1="MYPROMPT>\n"
bind '"z": "fj"'
bind -x '"f": READLINE_LINE="echo hellow"'
trap 'stty echo' DEBUG
bind '"j": accept-line'
