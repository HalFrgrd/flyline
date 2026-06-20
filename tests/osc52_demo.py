#!/usr/bin/env python3
import sys
import termios
import tty
import re
import atexit
import base64

# OSC 52 responses: \x1b]52;<clipboard_type>;<base64>\x07 OR \x1b]52;<clipboard_type>;<base64>\x1b\\
osc52_pattern = re.compile(r"\x1b\]52;([^;]*);([A-Za-z0-9+/=]*)(?:\x07|\x1b\\)")

def out(line: str) -> None:
    sys.stdout.write(line + "\r\n")
    sys.stdout.flush()

def restore():
    sys.stdout.write("\r\n")
    sys.stdout.flush()
    termios.tcsetattr(sys.stdin, termios.TCSADRAIN, orig)

orig = termios.tcgetattr(sys.stdin)
atexit.register(restore)

tty.setraw(sys.stdin.fileno())

out("=============================================================")
out("                  OSC 52 Clipboard Demo                      ")
out("=============================================================")
out("Instructions:")
out("  - Press 'c' to COPY 'Hello from OSC 52!' to the system clipboard")
out("  - Press 'p' to PASTE (query clipboard contents using OSC 52)")
out("  - Press 'q' or 'Ctrl-C' to QUIT")
out("-------------------------------------------------------------")

buf = ""

while True:
    c = sys.stdin.read(1)

    if not c:
        continue

    # Escape sequence parsing state machine
    if c == "\x1b":
        # Only reset buffer if we aren't already mid-sequence (so we don't break on \x1b\\ terminator)
        if not buf.startswith("\x1b]"):
            buf = "\x1b"
            continue

    if buf == "\x1b":
        if c == "]":
            buf = "\x1b]"
            continue
        else:
            buf = ""

    if buf.startswith("\x1b]"):
        buf += c
        # Search for matched OSC 52 response in buffer
        m = osc52_pattern.search(buf)
        if m:
            clip_type, b64_data = m.groups()
            try:
                decoded = base64.b64decode(b64_data).decode('utf-8')
                out(f"[PASTE RESPONSE] Decoded text: '{decoded}' (clipboard type: {clip_type})")
            except Exception as e:
                out(f"[PASTE RESPONSE] Failed to decode base64: {e} (raw b64: {b64_data})")
            buf = ""
        elif len(buf) > 2000:
            buf = ""
        continue

    # Normal keypresses (only reached when NOT parsing an escape sequence response)
    if c == "q" or c == "\x03":
        break

    if c == "c":
        # Copy text to clipboard
        text_to_copy = "Hello from OSC 52!"
        encoded = base64.b64encode(text_to_copy.encode('utf-8')).decode('utf-8')
        # OSC 52 write sequence
        copy_seq = f"\x1b]52;c;{encoded}\x07"
        sys.stdout.write(copy_seq)
        sys.stdout.flush()
        out(f"[COPY] Sent write sequence for: '{text_to_copy}'")
        continue

    if c == "p":
        # Query/read clipboard contents
        # OSC 52 read sequence ('?' requests data)
        query_seq = "\x1b]52;c;?\x07"
        sys.stdout.write(query_seq)
        sys.stdout.flush()
        out("[PASTE] Sent query request sequence: \\x1b]52;c;?\\x07")
        continue
