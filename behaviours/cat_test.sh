# run this:
strace cat
# you should see that there are no reads or writes until we press enter
# but there are still characters showing up on the screen


# running this you wont see any characters until you press enter
stty -echo; strace cat
# zsh will automatically turn echo back on when cat exits.
# so you can't run stty -echo on one command then run cat after it.
# bash will not, so you have to do it manually:
stty echo

