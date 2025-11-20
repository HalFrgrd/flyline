# Counter - Custom Bash Builtin

A simple custom bash builtin that acts as a persistent counter. This demonstrates how to create loadable bash builtins in C.

## Features

- **Increment**: Increase the counter by a specified amount (default: 1)
- **Decrement**: Decrease the counter by a specified amount (default: 1)
- **Set**: Set the counter to a specific value
- **Reset**: Reset the counter to 0
- **Get**: Display the current counter value

## Building

### Prerequisites

- GCC compiler
- Bash source code (included in `bash/` directory)

### Compile

```bash
cd custom_builtin
make
```

This will create `counter.so`, a shared library that can be dynamically loaded into bash.

## Usage

### Loading the Builtin

```bash
enable -f /path/to/counter.so counter
```

Or from the custom_builtin directory:

```bash
enable -f ./counter.so counter
```

### Commands

```bash
# Display current counter value
counter

# Increment by 1
counter inc

# Increment by n
counter inc 5

# Decrement by 1
counter dec

# Decrement by n
counter dec 3

# Set to specific value
counter set 42

# Reset to 0
counter reset

# Get current value (same as no arguments)
counter get
```

### Examples

```bash
# Basic usage
$ enable -f ./counter.so counter
$ counter
0
$ counter inc
1
$ counter inc 5
6
$ counter dec 2
4
$ counter set 100
100
$ counter reset
0

# Use in a loop
$ counter reset
0
$ for i in {1..5}; do counter inc; done
1
2
3
4
5

# Use in conditionals
$ counter set 10
10
$ if [ $(counter) -eq 10 ]; then echo "Counter is 10!"; fi
Counter is 10!
```

## Testing

Run the test script:

```bash
./test_counter.sh
```

Or use the Makefile:

```bash
make test
```

## How It Works

The counter builtin is implemented as a **loadable bash builtin**. This means:

1. It's compiled as a shared library (`.so` file)
2. It can be dynamically loaded into bash using the `enable -f` command
3. It maintains state (the counter value) within the bash process
4. It's much faster than shell functions or external commands

The implementation uses bash's builtin API defined in `loadables.h` and follows the same structure as bash's built-in commands.

## Implementation Details

- **Language**: C
- **Counter Storage**: Static variable (persists for the lifetime of the bash session)
- **Return Values**: 
  - `EXECUTION_SUCCESS` (0) on success
  - `EXECUTION_FAILURE` (1) on error
- **Thread Safety**: Not thread-safe (single bash process)

## File Structure

```
custom_builtin/
├── counter.c          # Counter builtin implementation
├── Makefile          # Build instructions
├── test_counter.sh   # Test script
└── README.md         # This file
```

## Cleaning Up

To remove built files:

```bash
make clean
```

To unload the builtin from bash:

```bash
enable -d counter
```

## License

This custom builtin follows the same GPL-3.0 license as bash itself.

## See Also

- Bash manual: `man bash` (search for "loadable builtins")
- Bash source: `examples/loadables/` directory
- `help counter` (after loading the builtin)
