#!/usr/bin/env python3
"""Verify that resizing on a direct terminal never scrolls the screen region.

To redraw after a window resize, Flyline first erases the inline viewport it
drew. It used to do that with DeleteLine (CSI M), which some terminals (foot)
implement as a rotation of their grid ring buffer: the screen looks unchanged,
but the terminal's viewport offset moves. The next reflow then leaves the cursor
below the reflowed prompt, so Flyline's cursor-up lands on the wrong row and the
old prompt is never erased - every resize stacks another prompt.

DeleteLine is only needed inside multiplexers, which keep their own line-wrap
state across redraws. A direct terminal must get the plain erase-display instead.
The test also verifies that a `ZELLIJ` session still gets the delete, so the
multiplexer workaround is preserved.
"""

import argparse
from collections.abc import Callable
import errno
import fcntl
import os
from pathlib import Path
import pty
import re
import select
import shlex
import signal
import struct
import sys
import tempfile
import termios
import time


DEVICE_ATTRIBUTES_QUERY = b"\x1b[>c"
DEVICE_ATTRIBUTES_RESPONSE = b"\x1b[>1;1;0c"
CURSOR_POSITION_QUERY = b"\x1b[6n"
CURSOR_POSITION_RESPONSE = b"\x1b[1;1R"
DELETE_LINE = re.compile(rb"\x1b\[[0-9]*M")
PROMPT_ENABLED = b"\x1b[?2004h"
CLEARED_ENVIRONMENT_VARIABLES = {
    "ZELLIJ",
    "ZELLIJ_SESSION_NAME",
    "TMUX",
    "STY",
    "BYOBU_PREFIX",
    "BYOBU_CONFIG_DIR",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "COLUMNS",
    "LINES",
}


def parse_arguments() -> argparse.Namespace:
    """Parse the Bash and Flyline paths used by the integration test."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--bash", default="/bin/bash", type=Path)
    parser.add_argument("--flyline", required=True, type=Path)
    return parser.parse_args()


def collect(terminal_fd: int, output: bytearray, seconds: float) -> None:
    """Drain terminal output for a while, answering DA and cursor position queries."""
    deadline = time.monotonic() + seconds
    scanned_from = len(output)
    answered_device_queries = 0
    answered_cursor_queries = 0

    while time.monotonic() < deadline:
        readable, _, _ = select.select([terminal_fd], [], [], 0.02)
        if readable:
            try:
                output.extend(os.read(terminal_fd, 65_536))
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                break

        device_queries = output.count(DEVICE_ATTRIBUTES_QUERY, scanned_from)
        for _ in range(device_queries - answered_device_queries):
            os.write(terminal_fd, DEVICE_ATTRIBUTES_RESPONSE)
        answered_device_queries = device_queries

        cursor_queries = output.count(CURSOR_POSITION_QUERY, scanned_from)
        for _ in range(cursor_queries - answered_cursor_queries):
            os.write(terminal_fd, CURSOR_POSITION_RESPONSE)
        answered_cursor_queries = cursor_queries


def collect_until(
    terminal_fd: int,
    output: bytearray,
    predicate: Callable[[], bool],
    seconds: float,
) -> bool:
    """Drain output until `predicate` is true or the deadline expires."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        collect(terminal_fd, output, 0.02)
        if predicate():
            return True
    return False


def resize_deletes_lines(
    bash_path: Path, flyline_path: Path, multiplexer: str | None
) -> bool | None:
    """Resize an interactive shell and report whether it erased with DeleteLine.

    Returns None when the shell never reached a prompt or never handled a
    resize, so the caller can tell a broken harness apart from a clean resize.
    """
    with tempfile.TemporaryDirectory(prefix="flyline-resize-test-") as temporary_directory:
        bashrc_path = Path(temporary_directory) / "bashrc"
        bashrc_path.write_text(
            "PS1='$ '\n"
            f"enable -f {shlex.quote(str(flyline_path))} flyline\n"
            "flyline --set-delayed-startup-ms 0\n",
            encoding="utf-8",
        )

        child_pid, terminal_fd = pty.fork()
        if child_pid == 0:
            environment = {
                key: value
                for key, value in os.environ.items()
                if key not in CLEARED_ENVIRONMENT_VARIABLES
            }
            environment |= {
                "COLUMNS": "80",
                "LINES": "24",
                "NO_COLOR": "1",
                "TERM": "foot",
            }
            if multiplexer is not None:
                environment[multiplexer] = "test"
            os.execve(
                bash_path,
                [str(bash_path), "--noprofile", "--rcfile", str(bashrc_path), "-i"],
                environment,
            )

        fcntl.ioctl(terminal_fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))

        output = bytearray()
        try:
            # Wait for the first prompt so the resize is handled by the line editor.
            if not collect_until(
                terminal_fd, output, lambda: PROMPT_ENABLED in output, 5.0
            ):
                return None

            # Let delayed startup finish before measuring the resize output.
            collect(terminal_fd, output, 0.5)
            output.clear()

            for columns in (40, 80, 40, 80):
                cursor_queries_before = output.count(CURSOR_POSITION_QUERY)
                fcntl.ioctl(
                    terminal_fd,
                    termios.TIOCSWINSZ,
                    struct.pack("HHHH", 24, columns, 0, 0),
                )
                # DeleteLine is emitted before the CPR query, so once the query
                # arrives any DL from this resize is already in the output.
                if not collect_until(
                    terminal_fd,
                    output,
                    lambda before=cursor_queries_before: (
                        output.count(CURSOR_POSITION_QUERY) > before
                    ),
                    2.0,
                ):
                    return None

            return DELETE_LINE.search(bytes(output)) is not None
        finally:
            try:
                os.kill(child_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.close(terminal_fd)
            os.waitpid(child_pid, 0)


def main() -> int:
    """Run the regression test and return a process exit status."""
    arguments = parse_arguments()

    direct_resize = resize_deletes_lines(arguments.bash, arguments.flyline, multiplexer=None)
    if direct_resize is None:
        print(
            "FAIL: Flyline did not render a prompt or handle a resize on a direct terminal",
            file=sys.stderr,
        )
        return 1
    if direct_resize:
        print("FAIL: resizing a direct terminal scrolled the screen with DeleteLine", file=sys.stderr)
        return 1

    zellij_resize = resize_deletes_lines(arguments.bash, arguments.flyline, multiplexer="ZELLIJ")
    if zellij_resize is None:
        print(
            "FAIL: Flyline did not render a prompt or handle a resize under zellij",
            file=sys.stderr,
        )
        return 1
    if not zellij_resize:
        print("FAIL: a zellij session no longer resets line-wrap state", file=sys.stderr)
        return 1

    print("PASS: resize erases without scrolling, except under zellij")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
