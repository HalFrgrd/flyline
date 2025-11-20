#!/bin/bash
# Test script for the counter builtin

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Counter Builtin Test ===${NC}\n"

# Load the counter builtin
echo "Loading counter builtin..."
enable -f ./counter.so counter

if ! type counter &>/dev/null; then
    echo "Error: Failed to load counter builtin"
    exit 1
fi

echo -e "${GREEN}✓ Counter builtin loaded${NC}\n"

# Test 1: Initial value
echo "Test 1: Show initial value"
echo -n "  counter = "
counter

# Test 2: Increment
echo -e "\nTest 2: Increment operations"
echo -n "  counter inc = "
counter inc

echo -n "  counter inc = "
counter inc

echo -n "  counter inc 5 = "
counter inc 5

# Test 3: Decrement
echo -e "\nTest 3: Decrement operations"
echo -n "  counter dec = "
counter dec

echo -n "  counter dec 3 = "
counter dec 3

# Test 4: Set value
echo -e "\nTest 4: Set operations"
echo -n "  counter set 100 = "
counter set 100

echo -n "  counter set 42 = "
counter set 42

# Test 5: Reset
echo -e "\nTest 5: Reset operation"
echo -n "  counter reset = "
counter reset

# Test 6: Use in a loop
echo -e "\nTest 6: Counter in a loop (counting to 10)"
counter reset > /dev/null
for i in {1..10}; do
    result=$(counter inc)
    echo -n "$result "
done
echo ""

# Test 7: Use with negative numbers
echo -e "\nTest 7: Negative numbers"
echo -n "  counter set -50 = "
counter set -50

echo -n "  counter inc 30 = "
counter inc 30

echo -n "  counter dec 10 = "
counter dec 10

# Test 8: Help documentation
echo -e "\n${BLUE}Test 8: Help documentation${NC}"
help counter

echo -e "\n${GREEN}=== All tests completed ===${NC}"
