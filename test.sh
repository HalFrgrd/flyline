#!/bin/bash

echo "Parent shell PID: $$"

run_jobu_bash_server() {
    echo "Function PID: $$"
}

export -f run_jobu_bash_server

coproc BASHSERVER { run_jobu_bash_server; }

echo "Coproc PID: ${BASHSERVER_PID}"  # Bash sets $BASHSERVER_PID
