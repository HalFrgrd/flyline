#!/usr/bin/env python3
"""Check that the final redraw emits the entire submitted command.

Runs real Bash and Flyline behind a PTY using only Python's standard library.
The Rust unit tests cover the prefix writer separately.
"""

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
import tempfile
import termios
import time


def check_submission(bash, library, columns=80, rows=24, count=210, edit_head=False):
    with tempfile.TemporaryDirectory(prefix="flyline-submit-test-") as temporary:
        directory = Path(temporary)
        destination = directory / "result"
        rc = directory / "bashrc"
        up = "+".join(["moveLineUp"] * 300)
        down = "+".join(["moveLineDown"] * 300)
        rc.write_text(
            "PS1='$ '\nPS1_FINAL='FINAL> '\n"
            f"HISTFILE={shlex.quote(str(directory / 'history'))}\n"
            f"enable -f {shlex.quote(str(library))} flyline || exit\n"
            "flyline --show-animations false --send-shell-integration-codes none "
            "--enable-easter-eggs false --set-delayed-startup-ms 0\n"
            "flyline mouse --mode disabled\n"
            f"flyline key bind Ctrl+g always={up}+moveLeftStartOfLine\n"
            f"flyline key bind Ctrl+t always={down}+moveRightEndOfLine\n"
        )
        pid, fd = pty.fork()
        if pid == 0:
            environment = os.environ.copy()
            for key in ("LD_PRELOAD", "BASH_ENV", "ENV", "PROMPT_COMMAND", "FLYLINE_VERSION"):
                environment.pop(key, None)
            environment.update(
                HOME=str(directory), XDG_DATA_HOME=str(directory / "data"),
                XDG_CACHE_HOME=str(directory / "cache"), XDG_STATE_HOME=str(directory / "state"),
                TERM="xterm-ghostty",
            )
            os.execve(str(bash), [str(bash), "--noprofile", "--rcfile", str(rc), "-i"], environment)

        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
        os.set_blocking(fd, False)
        output = bytearray()
        device_queries = cursor_queries = 0

        def pump(duration, until=None):
            nonlocal device_queries, cursor_queries
            deadline = time.monotonic() + duration
            while time.monotonic() < deadline:
                if select.select([fd], [], [], .01)[0]:
                    try:
                        data = os.read(fd, 262144)
                    except BlockingIOError:
                        continue
                    except OSError as error:
                        if error.errno == errno.EIO:
                            raise AssertionError("Bash exited unexpectedly") from error
                        raise
                    assert data, "Bash returned EOF unexpectedly"
                    output.extend(data)
                    current = output.count(b"\x1b[>c")
                    for _ in range(current - device_queries):
                        os.write(fd, b"\x1b[>1;10;0c")
                    device_queries = current
                    current = output.count(b"\x1b[6n")
                    for _ in range(current - cursor_queries):
                        os.write(fd, b"\x1b[1;1R")
                    cursor_queries = current
                if until is not None and until():
                    return

        def send(data, settle=.1):
            remaining = memoryview(data)
            deadline = time.monotonic() + 5
            while remaining:
                assert time.monotonic() < deadline, "PTY input write timed out"
                try:
                    remaining = remaining[os.write(fd, remaining):]
                except BlockingIOError:
                    pump(.01)
            pump(settle)

        try:
            pump(3, lambda: b"\x1b[?1004h" in output)
            assert b"\x1b[?2004h" in output, "Flyline did not enable bracketed paste"
            pump(.1)
            lines = [f"COMMIT_{index:04d}" for index in range(1, count + 1)]
            text = "\n".join(lines)
            command = f"printf %s {shlex.quote(text)} > {shlex.quote(str(destination))}"
            send(b"\x1b[200~" + command.encode() + b"\x1b[201~")
            pump(5, lambda: lines[-1].encode() in output)
            assert not destination.exists(), "paste ran without Enter"
            if count > rows:
                assert b"COMMIT_0001" not in output, "editing published a stale prefix into history"

            if edit_head:
                send(b"\x07")
                send(b"\x1b[C" * len("printf %s '") + b"EDIT_")
                text = "EDIT_" + text
                send(b"\x14")
            # Close automatic completion so Enter submits rather than accepts a menu.
            send(b"\x1b[27u")
            final_start = len(output)
            send(b"\r", .05)
            pump(5, destination.exists)
            assert destination.exists(), "command was not executed"
            pump(.2)
            assert destination.read_bytes() == text.encode(), "submitted command contents changed"
            # Inspect the submission frame only; the next prompt may repaint
            # matching history suggestions containing the same command text.
            frames = bytes(output[final_start:]).split(b"\x1b[?2026l")
            final_output = next((frame for frame in frames if b"FINAL>" in frame), b"")
            if count > rows:
                missing = [line for line in lines if line.encode() not in final_output]
                assert not missing, f"final redraw omitted {len(missing)} rows; first={missing[:1]}"
                assert all(final_output.count(line.encode()) == 1 for line in lines), "final redraw duplicated rows"
            assert b"FINAL>" in final_output, "final redraw lost the transient prompt"
            if edit_head:
                assert b"EDIT_COMMIT_0001" in final_output, "final redraw contains stale text"
            assert b"\x1b[3J" not in output, "terminal scrollback was erased"
        finally:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.close(fd)
            os.waitpid(pid, 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bash", type=Path, default=Path("/bin/bash"))
    parser.add_argument("--flyline", type=Path, required=True)
    args = parser.parse_args()
    cases = [("long", {}), ("edited-head", dict(edit_head=True)),
             ("narrow", dict(columns=40, rows=12)), ("short", dict(count=3))]
    failures = 0
    for name, options in cases:
        try:
            check_submission(args.bash.resolve(), args.flyline.resolve(), **options)
            print(f"PASS: submitted command scrollback ({name})", flush=True)
        except Exception as error:
            failures += 1
            print(f"FAIL: submitted command scrollback ({name}): {error}", flush=True)
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
