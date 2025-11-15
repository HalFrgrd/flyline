#!/bin/bash


read -r query

if [[ "$query" == "FOO" ]]; then
    echo "BAR"
    echo "bash server can still write to terminal if not careful. pid=$$"
else
    echo "Unexpected query: $query"
fi

sleep 1

echo "bash server exited"
