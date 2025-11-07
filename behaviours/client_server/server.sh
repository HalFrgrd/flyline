#!/bin/bash

coproc MYCOPROC {
    exec 3<&0 4>&1
    exec </dev/tty >/dev/tty
    bash client.sh;
    }

read -r query <&"${MYCOPROC[0]}"

if [[ "$query" == "FOO" ]]; then
    echo "BAR" >&"${MYCOPROC[1]}"
    echo "bash server can still write to terminal if not careful. pid=$$"
else
    echo "Unexpected query: $query" >&"${MYCOPROC[1]}"
fi

wait "${MYCOPROC_PID}"

echo "bash server exited"
