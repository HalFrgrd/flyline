#!/usr/bin/env python3
"""
A demo script showing off the Kitty terminal graphics protocol.
It prints 'hello world' and moves a tall vertical white cursor bar
smoothly over the text over 2 seconds using sub-column positioning.
"""

import sys
import time
import base64
import argparse
import termios
import tty
import atexit
import re
import select

def query_cell_size(timeout=0.15):
    """
    Query character cell size in pixels from terminal using CSI 16 t.
    Returns (width, height) or (None, None).
    """
    fd = sys.stdin.fileno()
    if not sys.stdin.isatty():
        return None, None
    orig = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        sys.stdout.write("\x1b[16t")
        sys.stdout.flush()
        
        # Wait for data
        rlist, _, _ = select.select([sys.stdin], [], [], timeout)
        if not rlist:
            return None, None
        buf = ""
        while len(buf) < 32:
            rlist, _, _ = select.select([sys.stdin], [], [], 0.02)
            if not rlist:
                break
            c = sys.stdin.read(1)
            buf += c
            if c == 't':
                break
        m = re.match(r".*?\x1b\[6;(\d+);(\d+)t", buf)
        if m:
            return int(m.group(2)), int(m.group(1))
    except Exception:
        pass
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, orig)
    return None, None

def query_cursor_position(timeout=0.15):
    """
    Query current cursor position using CSI 6 n.
    Returns (row, col) or (None, None).
    """
    fd = sys.stdin.fileno()
    if not sys.stdin.isatty():
        return None, None
    orig = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        sys.stdout.write("\x1b[6n")
        sys.stdout.flush()
        
        rlist, _, _ = select.select([sys.stdin], [], [], timeout)
        if not rlist:
            return None, None
        buf = ""
        while len(buf) < 32:
            rlist, _, _ = select.select([sys.stdin], [], [], 0.02)
            if not rlist:
                break
            c = sys.stdin.read(1)
            buf += c
            if c == 'R':
                break
        m = re.match(r".*?\x1b\[(\d+);(\d+)R", buf)
        if m:
            return int(m.group(1)), int(m.group(2))
    except Exception:
        pass
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, orig)
    return None, None

def main():
    parser = argparse.ArgumentParser(description="Kitty Image Protocol Cursor Demo")
    parser.add_argument("--text", type=str, default="hello world", help="Text to print")
    parser.add_argument("--cell-width", type=int, default=10, help="Fallback cell width in pixels")
    parser.add_argument("--cell-height", type=int, default=20, help="Fallback cell height in pixels")
    parser.add_argument("--cursor-width", type=int, default=2, help="Width of cursor in pixels")
    parser.add_argument("--duration", type=float, default=2.0, help="Animation duration in seconds")
    parser.add_argument("--fps", type=int, default=60, help="Frames per second")
    args = parser.parse_args()

    print("=============================================================")
    print("             Kitty Image Protocol Cursor Demo                ")
    print("=============================================================")
    print("Note: This script uses the Kitty graphics protocol to display")
    print("and animate a sub-column vertical cursor bar.")
    print("It works on supporting terminals (e.g. Kitty, WezTerm, Konsole, Ghostty).")
    print("-------------------------------------------------------------")

    # Detect cell size
    width, height = query_cell_size()
    cell_width = width if width else args.cell_width
    cell_height = height if height else args.cell_height

    print(f"Cell size: {cell_width}x{cell_height} pixels (using fallback if None)")
    print(f"Animating cursor over '{args.text}' over {args.duration}s...")
    time.sleep(1.0)

    # Detect cursor position
    row, col = query_cursor_position()
    if row is None or col is None:
        # Fallback to hardcoded row/col
        row, col = 12, 10
        # Let's ensure space exists
        sys.stdout.write("\n" * 15)
        sys.stdout.write(f"\x1b[{row};{col}H")
        sys.stdout.flush()

    # Hide cursor during the animation
    sys.stdout.write("\x1b[?25l")
    sys.stdout.flush()

    # Register cleanup to restore cursor and remove image
    def cleanup():
        sys.stdout.write("\x1b[?25h")  # Show cursor
        sys.stdout.write("\x1b_Ga=d,d=i,i=1,q=1;\x1b\\")  # Delete image ID 1
        sys.stdout.flush()

    atexit.register(cleanup)

    # Print the text at row, col
    sys.stdout.write(f"\x1b[{row};{col}H{args.text}")
    sys.stdout.flush()

    # Generate the image payload
    # Raw 32-bit RGBA image: cursor_width x cell_height
    # Opaque white pixels: \xff\xff\xff\xff (Red, Green, Blue, Alpha)
    raw_rgba = b"\xff\xff\xff\xff" * (args.cursor_width * cell_height)
    payload = base64.b64encode(raw_rgba).decode('utf-8')

    # Transmit the image to the terminal (action a=t: transmit only, don't place yet)
    # image ID = 1
    sys.stdout.write(f"\x1b_Ga=t,f=32,s={args.cursor_width},v={cell_height},i=1,q=1;{payload}\x1b\\")
    sys.stdout.flush()

    # Run the animation
    start_time = time.time()
    total_pixels = len(args.text) * cell_width
    dt = 1.0 / args.fps

    while True:
        now = time.time()
        elapsed = now - start_time
        if elapsed >= args.duration:
            break
        
        progress = elapsed / args.duration
        current_pixel = int(progress * total_pixels)
        
        col_offset = current_pixel // cell_width
        sub_pixel_x = current_pixel % cell_width
        
        target_col = col + col_offset
        
        # Position terminal cursor at the target cell.
        # Place the image (action a=p) referencing image ID 1 and placement ID 1.
        # We specify z=1 to render the image above the text layer.
        # We specify r=1 to size the image to exactly one character cell high.
        # We specify q=1 to suppress OK responses from the terminal.
        sys.stdout.write(f"\x1b[{row};{target_col}H")
        sys.stdout.write(f"\x1b_Ga=p,i=1,p=1,z=1,r=1,q=1,X={sub_pixel_x},Y=0;\x1b\\")
        sys.stdout.flush()
        
        time.sleep(dt)

    # Place at the very end
    sys.stdout.write(f"\x1b[{row};{col + len(args.text)}H")
    sys.stdout.write(f"\x1b_Ga=p,i=1,p=1,z=1,r=1,q=1,X=0,Y=0;\x1b\\")
    sys.stdout.flush()
    time.sleep(0.1)

    # Move cursor to the line below the text for clean exit
    sys.stdout.write(f"\x1b[{row + 1};1H\n")
    sys.stdout.flush()

if __name__ == "__main__":
    main()
