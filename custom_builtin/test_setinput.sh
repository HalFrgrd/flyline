#!/bin/bash
enable -f ./counter.so counter
echo "Calling counter setinput..."
counter setinput
echo "Done! No segfault."
