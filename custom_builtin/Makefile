# Makefile for counter loadable builtin
#
# Usage:
#   make          - Build counter.so
#   make test     - Build and test the builtin
#   make clean    - Clean build artifacts

BASH_DIR = bash
LOADABLES_DIR = $(BASH_DIR)/examples/loadables

# Compiler and flags
CC = gcc
CFLAGS = -fPIC -DHAVE_CONFIG_H -DSHELL
CFLAGS += -I$(BASH_DIR) -I$(BASH_DIR)/include -I$(BASH_DIR)/lib -I$(BASH_DIR)/builtins -I$(LOADABLES_DIR)
LDFLAGS = -shared

# Target
TARGET = counter.so

# Source files
SRC = counter.c

all: $(TARGET)

$(TARGET): $(SRC)
	@echo "Building counter loadable builtin..."
	$(CC) $(CFLAGS) $(LDFLAGS) -o $(TARGET) $(SRC)
	@echo "Built $(TARGET) successfully!"
	@echo ""
	@echo "To use in bash:"
	@echo "  enable -f $(PWD)/$(TARGET) counter"
	@echo "  counter          # Show current value"
	@echo "  counter inc      # Increment by 1"
	@echo "  counter inc 5    # Increment by 5"
	@echo "  counter dec      # Decrement by 1"
	@echo "  counter set 42   # Set to 42"
	@echo "  counter reset    # Reset to 0"

test: $(TARGET)
	@echo "Testing counter builtin..."
	@bash -c "enable -f ./$(TARGET) counter && \
	          echo 'Initial value:' && counter && \
	          echo 'After inc:' && counter inc && \
	          echo 'After inc 5:' && counter inc 5 && \
	          echo 'After dec:' && counter dec && \
	          echo 'After set 100:' && counter set 100 && \
	          echo 'After dec 10:' && counter dec 10 && \
	          echo 'After reset:' && counter reset"

clean:
	rm -f $(TARGET) *.o

.PHONY: all test clean
