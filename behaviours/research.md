# Terminal Control Codes Reference

## ANSI Escape Sequences

### Cursor Control
```bash
# Move cursor
\e[{row};{col}H      # Move to specific position (1-based)
\e[{row};{col}f      # Alternative move to position
\e[{n}A              # Move up n lines
\e[{n}B              # Move down n lines  
\e[{n}C              # Move right n columns
\e[{n}D              # Move left n columns
\e[H                 # Move to home (1,1)
\e[0;0H              # Move to top-left corner

# Cursor save/restore
\e[s                 # Save cursor position (SCO)
\e[u                 # Restore cursor position (SCO)
\e7                  # Save cursor position (DEC)
\e8                  # Restore cursor position (DEC)

# Cursor visibility
\e[?25h              # Show cursor
\e[?25l              # Hide cursor
```

### Screen/Line Clearing
```bash
\e[2J                # Clear entire screen
\e[1J                # Clear from cursor to beginning of screen
\e[0J                # Clear from cursor to end of screen
\e[3J                # Clear entire screen + scrollback
\e[K                 # Clear from cursor to end of line
\e[1K                # Clear from cursor to beginning of line
\e[2K                # Clear entire line
```

### Colors and Text Formatting
```bash
# Foreground colors (30-37, 90-97)
\e[30m               # Black
\e[31m               # Red
\e[32m               # Green
\e[33m               # Yellow
\e[34m               # Blue
\e[35m               # Magenta
\e[36m               # Cyan
\e[37m               # White
\e[90m               # Bright black (gray)
\e[91m               # Bright red
\e[92m               # Bright green
\e[93m               # Bright yellow
\e[94m               # Bright blue
\e[95m               # Bright magenta
\e[96m               # Bright cyan
\e[97m               # Bright white

# Background colors (40-47, 100-107)
\e[40m               # Black background
\e[41m               # Red background
\e[42m               # Green background
# ... same pattern as foreground

# 256 colors
\e[38;5;{n}m         # Set foreground to color n (0-255)
\e[48;5;{n}m         # Set background to color n (0-255)

# True color (24-bit)
\e[38;2;{r};{g};{b}m # Set foreground RGB
\e[48;2;{r};{g};{b}m # Set background RGB

# Text attributes
\e[0m                # Reset all attributes
\e[1m                # Bold/bright
\e[2m                # Dim
\e[3m                # Italic
\e[4m                # Underline
\e[5m                # Blink
\e[7m                # Reverse/invert
\e[8m                # Hidden/invisible
\e[9m                # Strikethrough
\e[21m               # Reset bold
\e[22m               # Reset dim
\e[23m               # Reset italic
\e[24m               # Reset underline
\e[25m               # Reset blink
\e[27m               # Reset reverse
\e[28m               # Reset hidden
\e[29m               # Reset strikethrough
```

### Mouse Support
```bash
# Mouse tracking modes
\e[?1000h            # Enable basic mouse tracking (click)
\e[?1000l            # Disable basic mouse tracking
\e[?1002h            # Enable button event tracking (drag)
\e[?1002l            # Disable button event tracking
\e[?1003h            # Enable any event tracking (movement)
\e[?1003l            # Disable any event tracking
\e[?1006h            # Enable SGR extended mouse mode
\e[?1006l            # Disable SGR extended mouse mode
\e[?1015h            # Enable urxvt extended mouse mode
\e[?1015l            # Disable urxvt extended mouse mode

# Mouse wheel support
\e[?1007h            # Enable alternate scroll mode
\e[?1007l            # Disable alternate scroll mode
```

### Alternative Screen Buffer
```bash
\e[?1049h            # Enable alternative screen + save cursor
\e[?1049l            # Disable alternative screen + restore cursor
\e[?47h              # Enable alternative screen buffer
\e[?47l              # Disable alternative screen buffer
```

### Terminal Status Queries
```bash
# Device Status Report (DSR)
\e[6n                # Query cursor position (returns \e[{row};{col}R)
\e[5n                # Query device status (returns \e[0n or \e[3n)

# Terminal identification
\e[c                 # Primary Device Attributes (what am I?)
\e[>c                # Secondary Device Attributes (version info)
\e[0c                # Reset then query primary attributes

# Terminal size queries
\e[18t               # Query text area size (returns \e[8;{rows};{cols}t)
\e[19t               # Query screen size in pixels
\e[14t               # Query window size in pixels

# Window title queries
\e[21t               # Query window title (returns \e]l{title}\e\\)
```

### Scrolling Control
```bash
\e[{n}S              # Scroll up n lines
\e[{n}T              # Scroll down n lines
\eM                  # Reverse index (scroll down one line)
\eD                  # Index (scroll up one line)

# Scroll regions
\e[{top};{bottom}r   # Set scroll region (top and bottom line numbers)
\e[r                 # Reset scroll region to full screen
```

### Keyboard/Input Control
```bash
\e[?1h               # Enable application cursor keys
\e[?1l               # Disable application cursor keys
\e[?25h              # Show cursor
\e[?25l              # Hide cursor
\e[?2004h            # Enable bracketed paste mode
\e[?2004l            # Disable bracketed paste mode
```

## Portable Alternatives with tput

### Cursor Movement
```bash
tput cup {row} {col} # Move cursor to position (0-based)
tput home            # Move to home position
tput cuu {n}         # Move up n lines
tput cud {n}         # Move down n lines
tput cuf {n}         # Move right n columns
tput cub {n}         # Move left n columns
```

### Cursor Save/Restore
```bash
tput sc              # Save cursor position
tput rc              # Restore cursor position
```

### Screen Clearing
```bash
tput clear           # Clear screen
tput ed              # Clear to end of screen
tput el              # Clear to end of line
tput el1             # Clear to beginning of line
```

### Colors and Attributes
```bash
tput setaf {n}       # Set foreground color (0-7)
tput setab {n}       # Set background color (0-7)
tput bold            # Enable bold
tput dim             # Enable dim
tput smul            # Start underline
tput rmul            # End underline
tput rev             # Reverse video
tput sgr0            # Reset all attributes
tput reset           # Reset terminal completely

# Color numbers for tput setaf/setab:
# 0=black, 1=red, 2=green, 3=yellow, 4=blue, 5=magenta, 6=cyan, 7=white
```

### Terminal Information
```bash
tput cols            # Get number of columns
tput lines           # Get number of lines
tput colors          # Get number of colors supported
```

### Cursor Visibility
```bash
tput civis           # Hide cursor
tput cnorm           # Show cursor (normal)
tput cvvis           # Show cursor (very visible)
```

## Portable Alternatives with stty

### Terminal Settings
```bash
stty -echo           # Disable echo
stty echo            # Enable echo
stty raw             # Raw mode (no processing)
stty cooked          # Cooked mode (normal processing)
stty -icanon         # Disable canonical mode (read char by char)
stty icanon          # Enable canonical mode (line by line)
stty size            # Get terminal size (rows cols)
stty -a              # Show all settings
```

### Special Characters
```bash
stty intr '^C'       # Set interrupt character
stty eof '^D'        # Set EOF character
stty erase '^?'      # Set backspace character
stty kill '^U'       # Set line kill character
```

## Shell Variables and Commands

### Terminal Size
```bash
echo $COLUMNS $LINES # Shell variables (may not be current)
resize               # Update COLUMNS and LINES (if available)
```

### Terminal Type Detection
```bash
echo $TERM           # Terminal type
infocmp              # Terminal capabilities
```

## Cross-Platform Considerations

### Reliable Detection
```bash
# Check for specific capabilities
tput colors >/dev/null 2>&1 && echo "Color supported"
tput cup 0 0 >/dev/null 2>&1 && echo "Cursor positioning supported"

# Feature detection
case "$TERM" in
    xterm*|screen*|tmux*)
        # Full featured terminal
        ;;
    vt100|vt102)
        # Basic terminal
        ;;
    dumb)
        # No special features
        ;;
esac
```

### Safe Fallbacks
```bash
# Safe color function
color_red() {
    if [ -t 1 ] && tput setaf 1 >/dev/null 2>&1; then
        tput setaf 1
    fi
}

# Safe cursor positioning
move_cursor() {
    local row=$1 col=$2
    if tput cup "$row" "$col" >/dev/null 2>&1; then
        tput cup "$row" "$col"
    else
        printf '\e[%d;%dH' "$((row + 1))" "$((col + 1))"
    fi
}
```

## Example Usage in Scripts

### Progress Bar with Colors
```bash
#!/bin/bash
progress_bar() {
    local current=$1 total=$2 width=50
    local percent=$((current * 100 / total))
    local filled=$((current * width / total))
    
    tput setaf 2  # Green
    printf "["
    printf "%*s" $filled | tr ' ' '='
    tput setaf 1  # Red
    printf "%*s" $((width - filled)) | tr ' ' '-'
    tput setaf 2  # Green
    printf "] %d%%\r" $percent
    tput sgr0     # Reset
}
```

### Interactive Menu
```bash
#!/bin/bash
show_menu() {
    tput clear
    tput cup 2 5
    tput setaf 6  # Cyan
    echo "=== Main Menu ==="
    tput sgr0
    
    local options=("Option 1" "Option 2" "Exit")
    local selected=0
    
    while true; do
        for i in "${!options[@]}"; do
            tput cup $((4 + i)) 5
            if [ $i -eq $selected ]; then
                tput rev  # Reverse video
            fi
            echo "${options[$i]}"
            tput sgr0
        done
        
        # Read single character
        read -rsn1 key
        case "$key" in
            $'\x1b')  # Escape sequence
                read -rsn2 key
                case "$key" in
                    '[A') ((selected--)) ;;  # Up arrow
                    '[B') ((selected++)) ;;  # Down arrow
                esac
                ;;
            '') break ;;  # Enter
        esac
        
        # Wrap selection
        [ $selected -lt 0 ] && selected=$((${#options[@]} - 1))
        [ $selected -ge ${#options[@]} ] && selected=0
    done
    
    echo "Selected: ${options[$selected]}"
}