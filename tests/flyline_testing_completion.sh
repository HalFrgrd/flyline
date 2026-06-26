#!/usr/bin/env bash

# This script registers a programmable completion function for `flyline_testing`
# so that running tab completion on `flyline_testing --big ` returns 10,000 entries.

_flyline_testing_completions() {
    local cur prev
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    if [[ "$prev" == "--big" ]]; then
        # Generate 10,000 entries: --entry-0 through --entry-9999
        local entries=()
        for i in {0..399999}; do
            entries+=("--asdfasdfasdfweoriuweioruentry-$i")
        done
        COMPREPLY=( $(compgen -W "${entries[*]}" -- "$cur") )
        return 0
    fi

    COMPREPLY=( $(compgen -W "--big" -- "$cur") )
}

complete -F _flyline_testing_completions flyline_testing
