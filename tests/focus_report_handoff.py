#!/usr/bin/env python3
"""Verify that terminal focus reports cannot leak across Flyline prompt handoffs."""

import argparse
import errno
import fcntl
import os
from pathlib import Path
import pty
import select
import shlex
import signal
import struct
import sys
import tempfile
import termios
import time


DEVICE_ATTRIBUTES_QUERY = b"\x1b[>c"
DEVICE_ATTRIBUTES_RESPONSE = b"\x1b[>0;1;0c"
CURSOR_POSITION_QUERY = b"\x1b[6n"
CURSOR_POSITION_RESPONSE = b"\x1b[1;1R"
FOCUS_TRACKING_ENABLED = b"\x1b[?1004h"
FOCUS_TRACKING_DISABLED = b"\x1b[?1004l"
FOCUS_IN = b"\x1b[I"
FOCUS_IN_ECHO = b"^[[I"
STARTUP_MARKER = b"[flyline inserted newline]"


def parse_arguments() -> argparse.Namespace:
    """Parse the Bash and Flyline paths used by the integration test."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--bash", default="/bin/bash", type=Path)
    parser.add_argument("--flyline", required=True, type=Path)
    return parser.parse_args()


def run_handoff_test(bash_path: Path, flyline_path: Path) -> str | None:
    """Return the visible leak description, or None when the handoff is clean."""
    # Create an isolated Bash configuration that starts focus tracking immediately.
    with tempfile.TemporaryDirectory(prefix="flyline-focus-test-") as temporary_directory:
        bashrc_path = Path(temporary_directory) / "bashrc"
        bashrc_path.write_text(
            f"PS1='$ '\n"
            f"enable -f {shlex.quote(str(flyline_path))} flyline\n"
            "flyline --set-delayed-startup-ms 0\n",
            encoding="utf-8",
        )

        # Start Bash behind a real pseudo-terminal so its line discipline can expose the echo race.
        child_pid, terminal_fd = pty.fork()
        if child_pid == 0:
            environment = os.environ | {
                "COLUMNS": "80",
                "LINES": "24",
                "NO_COLOR": "1",
                "TERM": "xterm-ghostty",
            }
            os.execve(
                bash_path,
                [
                    str(bash_path),
                    "--noprofile",
                    "--rcfile",
                    str(bashrc_path),
                    "-i",
                ],
                environment,
            )

        # Emulate the terminal protocol and record the raw stream around one empty submission.
        fcntl.ioctl(terminal_fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        deadline = time.monotonic() + 3
        output = bytearray()
        handled_device_queries = 0
        handled_cursor_queries = 0
        submission_offset: int | None = None
        injected_at: float | None = None
        injection_offset: int | None = None

        try:
            # Deliver FocusIn at the handoff boundary where a focused terminal can race Flyline's shutdown.
            while time.monotonic() < deadline:
                readable, _, _ = select.select([terminal_fd], [], [], 0.05)
                if readable:
                    try:
                        output.extend(os.read(terminal_fd, 65_536))
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                        break

                # Answer each terminal query once so delayed startup remains deterministic and fast.
                device_queries = output.count(DEVICE_ATTRIBUTES_QUERY)
                for _ in range(device_queries - handled_device_queries):
                    os.write(terminal_fd, DEVICE_ATTRIBUTES_RESPONSE)
                handled_device_queries = device_queries

                cursor_queries = output.count(CURSOR_POSITION_QUERY)
                for _ in range(cursor_queries - handled_cursor_queries):
                    os.write(terminal_fd, CURSOR_POSITION_RESPONSE)
                handled_cursor_queries = cursor_queries

                # Submit an empty command, then respond at the exact focus-mode shutdown boundary.
                if submission_offset is None and FOCUS_TRACKING_ENABLED in output:
                    os.write(terminal_fd, b"\r")
                    submission_offset = len(output)
                if (
                    submission_offset is not None
                    and injected_at is None
                    and output.find(FOCUS_TRACKING_DISABLED, submission_offset) >= 0
                ):
                    os.write(terminal_fd, FOCUS_IN)
                    injected_at = time.monotonic()
                    injection_offset = len(output)

                # Stop once the next prompt has had enough time to render or the visible leak appears.
                focus_echo_index = output.find(FOCUS_IN_ECHO, injection_offset or 0)
                marker_index = output.find(STARTUP_MARKER, focus_echo_index + len(FOCUS_IN_ECHO))
                if focus_echo_index >= 0 and marker_index >= 0:
                    return "^[[I[flyline inserted newline]"
                if injected_at is not None and time.monotonic() - injected_at >= 1:
                    break
        finally:
            # Terminate and reap the disposable shell even when an assertion fails.
            try:
                os.kill(child_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.close(terminal_fd)
            os.waitpid(child_pid, 0)

    # Distinguish an incomplete harness run from a clean handoff.
    if submission_offset is None:
        return "Flyline did not enable focus tracking"
    if injected_at is None:
        return "Flyline did not disable focus tracking after Enter"
    return None


def main() -> int:
    """Run the integration test and return a process exit status."""
    arguments = parse_arguments()

    # Report the user-visible failure without exposing terminal control bytes in CI logs.
    failure = run_handoff_test(arguments.bash, arguments.flyline)
    if failure is not None:
        print(f"FAIL: {failure}", file=sys.stderr)
        return 1

    print("PASS: FocusIn remained invisible during the prompt handoff")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
