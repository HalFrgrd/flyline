#!/usr/bin/env python3
"""Verify that unchanged Flyline redraws emit no empty synchronized updates."""

import argparse
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


BEGIN_UPDATE = b"\x1b[?2026h"
END_UPDATE = b"\x1b[?2026l"
PROMPT = b"sync-test> "
QUERIES = {
    b"\x1b[>c": b"\x1b[>0;1;0c",
    b"\x1b[6n": b"\x1b[1;1R",
}


def run_test(bash_path: Path, flyline_path: Path) -> str | None:
    with tempfile.TemporaryDirectory(prefix="flyline-sync-test-") as directory:
        bashrc = Path(directory) / "bashrc"
        bashrc.write_text(
            "PS1='sync-test> '\n"
            f"enable -f {shlex.quote(str(flyline_path))} flyline || exit\n"
            "flyline --show-animations false --set-frame-rate 10 --set-delayed-startup-ms 0 || exit\n"
            "flyline set-cursor --backend terminal || exit\n",
            encoding="utf-8",
        )
        child_pid, terminal_fd = pty.fork()
        if child_pid == 0:
            environment = os.environ | {
                "HOME": directory,
                "HISTFILE": "/dev/null",
                "COLUMNS": "80",
                "LINES": "24",
                "NO_COLOR": "1",
                "TERM": "xterm-ghostty",
            }
            os.chdir(directory)
            os.execve(
                bash_path,
                [str(bash_path), "--noprofile", "--rcfile", str(bashrc), "-i"],
                environment,
            )

        fcntl.ioctl(terminal_fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        output = bytearray()
        handled_queries = {query: 0 for query in QUERIES}
        ready_at = None
        input_at = None
        input_offset = None
        deadline = time.monotonic() + 5
        try:
            while time.monotonic() < deadline:
                readable, _, _ = select.select([terminal_fd], [], [], 0.05)
                if readable:
                    try:
                        block = os.read(terminal_fd, 65_536)
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                        break
                    if not block:
                        break
                    output.extend(block)
                for query, response in QUERIES.items():
                    count = output.count(query)
                    for _ in range(count - handled_queries[query]):
                        os.write(terminal_fd, response)
                    handled_queries[query] = count
                now = time.monotonic()
                if ready_at is None and PROMPT in output and END_UPDATE in output:
                    ready_at = now
                if ready_at is not None and input_at is None and now - ready_at >= 0.5:
                    input_offset = len(output)
                    os.write(terminal_fd, b"Z")
                    input_at = now
                if input_at is not None and now - input_at >= 1:
                    break
        finally:
            try:
                os.kill(child_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.close(terminal_fd)
            os.waitpid(child_pid, 0)

    if ready_at is None or input_offset is None:
        return "Flyline did not render the initial synchronized prompt"
    frames = re.findall(re.escape(BEGIN_UPDATE) + b"(.*?)" + re.escape(END_UPDATE), output, re.S)
    empty_frames = sum(not frame for frame in frames)
    if empty_frames:
        return f"emitted {empty_frames} empty synchronized updates ({len(frames)} total frames)"
    edited_frames = re.findall(
        re.escape(BEGIN_UPDATE) + b"(.*?)" + re.escape(END_UPDATE), output[input_offset:], re.S
    )
    if not any(b"Z" in frame for frame in edited_frames):
        return "typed input was not rendered inside a synchronized update"
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bash", type=Path, default=Path("/bin/bash"))
    parser.add_argument("--flyline", type=Path, required=True)
    arguments = parser.parse_args()
    failure = run_test(arguments.bash.resolve(), arguments.flyline.resolve())
    if failure is not None:
        print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    print("PASS: unchanged redraws are silent; typed input remains synchronized")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
