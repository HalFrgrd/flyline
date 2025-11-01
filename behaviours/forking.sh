#!/bin/bash

# Set up shared environment
SHARED_VAR="shared data2"
shared_function() {
    echo "Shared function called with: $1"
}
# export -f shared_function

# Fork process that inherits environment
fork_with_environment() {
    {
        echo "Fork has access to SHARED_VAR: $SHARED_VAR"
        shared_function "from fork"
        
        # Fork can also modify its own environment
        LOCAL_VAR="fork local"
        echo "Fork local var: $LOCAL_VAR"
    } &
    
    local fork_pid=$!
    echo "Forked process PID: $fork_pid"
    wait $fork_pid
}

fork_with_environment