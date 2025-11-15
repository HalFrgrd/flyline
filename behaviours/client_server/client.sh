#!/bin/bash


# at start up, stdin and stdout are pipes to the parent process (the client)
# we still want to use them for communication with the server, but we also want
# to be able to read and write to the terminal.

coproc MYCOPROC {
    "$(dirname "$0")/server.sh";
    }

coprocpid=${MYCOPROC_PID}
echo "client direct to terminal: hey there from client pid $$"
echo "FOO" >&${MYCOPROC[1]}

read -r response <&${MYCOPROC[0]}
echo "client received response from server: $response"

while true; do
    IFS= read -r -n1 from_tty
    hex=$(printf '%02x' "'$from_tty")
    echo "client read from tty: $hex"
    if [[ "$hex" == "71" ]]; then # 'q' to quit
        echo "client exiting on 'q'"
        break
    fi
done
echo "coprocpid=$coprocpid"
wait "$coprocpid"
