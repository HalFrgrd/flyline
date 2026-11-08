#!/usr/bin/env python3
"""Exercise completion cycling in a real Bash PTY, without Docker.

Usage: python3 tests/tab_cycle_pty.py --flyline target/release/libflyline.so
"""

import argparse
import errno
import fcntl
from functools import partial
import os
from pathlib import Path
import pty
import re
import select
import shlex
import signal
import struct
import tempfile
import termios
import time


# Flyline enables CSI-u key disambiguation. Avoid the legacy Escape/Alt timeout.
ESCAPE = b"\x1b[27u"


class Shell:
    def __init__(
        self, library, directory, bash_path, *, automatic=False, inline=False, delay=0
    ):
        self.output = bytearray()
        self.queries = {b"\x1b[>c": 0, b"\x1b[6n": 0}
        self.requests = directory / "requests"
        rcfile = directory / "bashrc"
        rcfile.write_text(
            "PS1='$ '\n"
            "HISTFILE=/dev/null\n"
            "__cycle() { printf '\\nCYCLE_RESULT:<%s>\\n' \"$*\"; }\n"
            "__complete_cycle() {\n"
            f"  printf x >> {shlex.quote(str(self.requests))}\n"
            f"  sleep {delay}\n"
            "  COMPREPLY=()\n"
            "  local value\n"
            "  for value in foo foobar; do\n"
            '    [[ $value == "${COMP_WORDS[COMP_CWORD]}"* ]] && COMPREPLY+=("$value")\n'
            "  done\n"
            "  return 0\n"
            "}\n"
            "complete -o nospace -F __complete_cycle __cycle\n"
            f"enable -f {shlex.quote(str(library))} flyline || exit 1\n"
            "flyline --set-delayed-startup-ms 0\n"
            "flyline mouse --mode disabled\n"
            "flyline suggestions --sort-order alphabetical "
            f"--auto-suggest {str(automatic).lower()} "
            f"--auto-suggest-inline {str(inline).lower()}\n"
            "flyline key bind Tab 'editingBufferMode=tabCompletionCycleInsert'\n"
            "flyline key bind Tab 'tabCompletionBackgroundActive=tabCompletionCycleInsert'\n"
            "flyline key bind Tab 'tabCompletionWaiting=tabCompletionCycleInsert'\n"
            "flyline key bind Tab 'tabCompletionAvailable=tabCompletionCycleInsert'\n"
            "flyline key bind Enter 'tabCompletionInserted=submitOrNewline'\n"
            "flyline key bind Ctrl+g 'always=submitOrNewline'\n"
            "flyline key bind Ctrl+y 'tabCompletionAvailable=tabCompletionAcceptAll'\n",
            encoding="utf-8",
        )
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            environment = os.environ | {
                "TERM": "xterm-256color",
                "COLUMNS": "100",
                "LINES": "30",
                "XDG_DATA_HOME": str(directory / "data"),
                "XDG_CACHE_HOME": str(directory / "cache"),
                "XDG_CONFIG_HOME": str(directory / "config"),
                "XDG_STATE_HOME": str(directory / "state"),
            }
            os.chdir(directory)
            os.execve(
                bash_path,
                [str(bash_path), "--noprofile", "--rcfile", str(rcfile), "-i"],
                environment,
            )
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))

    def pump(self, duration):
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            timeout = min(0.02, max(0, deadline - time.monotonic()))
            ready, _, _ = select.select([self.fd], [], [], timeout)
            if ready:
                try:
                    data = os.read(self.fd, 65536)
                except OSError as error:
                    if error.errno == errno.EIO:
                        raise AssertionError(f"Bash exited: {bytes(self.output[-1000:])!r}") from error
                    raise
                if not data:
                    raise AssertionError("Bash closed its terminal")
                self.output.extend(data)
            for query, response in [
                (b"\x1b[>c", b"\x1b[>0;1;0c"),
                (b"\x1b[6n", b"\x1b[1;1R"),
            ]:
                count = self.output.count(query)
                for _ in range(count - self.queries[query]):
                    os.write(self.fd, response)
                self.queries[query] = count

    def send(self, keys, wait=0.15):
        os.write(self.fd, keys)
        self.pump(wait)

    def expect_result(self, value):
        expected = f"CYCLE_RESULT:<{value}>".encode()
        deadline = time.monotonic() + 3
        while expected not in self.output and time.monotonic() < deadline:
            self.pump(0.05)
        visible = re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", self.output)
        visible = b" ".join(visible.split())[-1000:]
        results = re.findall(rb"\r?\nCYCLE_RESULT:<([^\r\n]*)>\r?\n", self.output)
        assert results == [value.encode()], (
            f"Expected only {expected!r}; terminal tail: {visible!r}"
        )

    def close(self):
        try:
            os.kill(self.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        os.close(self.fd)
        os.waitpid(self.pid, 0)


def run_case(
    library, name, steps, expected, *, bash_path, request_count=None, **options
):
    with tempfile.TemporaryDirectory(prefix="flyline-cycle-test-") as directory:
        shell = Shell(library, Path(directory), bash_path, **options)
        try:
            deadline = time.monotonic() + 3
            while b"\x1b[?2004h" not in shell.output and time.monotonic() < deadline:
                shell.pump(0.02)
            assert b"\x1b[?2004h" in shell.output, "Flyline did not initialize the terminal"
            shell.pump(0.15)
            for index, (keys, wait) in enumerate(steps):
                shell.send(keys, wait)
                if index == 0 and options.get("delay"):
                    deadline = time.monotonic() + 3
                    while not shell.requests.exists() and time.monotonic() < deadline:
                        shell.pump(0.01)
                    assert shell.requests.exists(), "Completion process did not start"
            shell.expect_result(expected)
            if request_count is not None:
                assert shell.requests.read_text() == "x" * request_count, (
                    "Unexpected completion process count"
                )
            print(f"PASS: {name}", flush=True)
        finally:
            shell.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bash", type=Path, default=Path("/bin/bash"))
    parser.add_argument("--flyline", type=Path, required=True)
    arguments = parser.parse_args()
    run = partial(
        run_case,
        arguments.flyline.resolve(strict=True),
        bash_path=arguments.bash.resolve(strict=True),
    )
    run(
        "cycle then Enter",
        [(b"__cycle f", 0.15), (b"\t", 0.2), (b"\t", 0.1), (b"\r", 0.1)],
        "foobar",
    )
    run(
        "type after insertion",
        [(b"__cycle f\t", 0.2), (b"b", 0.1), (b"\t", 0.2), (b"\x07", 0.1)],
        "foobar",
    )
    run(
        "accept all after insertion",
        [(b"__cycle f\t", 0.2), (b"\x19", 0.1), (b"\x07", 0.1)],
        "foo foobar",
    )
    run(
        "navigate then accept",
        [(b"__cycle f\t", 0.2), (b"\x1b[B", 0.1), (b"\r", 0.1), (b"\x07", 0.1)],
        "foobar",
    )
    run(
        "hidden background completion",
        [(b"__cycle f", 0.3), (b"\t", 0.15), (b"\t", 0.1), (b"\r", 0.1)],
        "foobar",
        inline=True,
    )
    run(
        "repeated Tab while loading",
        [(b"__cycle f\t", 0.05), (b"\t", 0.5), (b"\r", 0.1)],
        "foo",
        delay=0.35,
        request_count=1,
    )
    run(
        "Escape cancels pending insertion",
        [(b"__cycle f\t", 0.05), (ESCAPE, 0.5), (b"\x07", 0.1)],
        "f",
        delay=0.35,
    )
    run(
        "typing cancels pending insertion",
        [(b"__cycle f\t", 0.05), (b"b", 0.5), (b"\x07", 0.1)],
        "fb",
        delay=0.35,
    )
    run(
        "cursor movement cancels pending insertion",
        [(b"__cycle f\t", 0.05), (b"\x1b[D", 0.5), (b"\x07", 0.1)],
        "f",
        delay=0.35,
    )
    run(
        "Escape retains inserted text",
        [(b"__cycle f", 0.3), (b"\t", 0.2), (ESCAPE, 0.2), (b"\x07", 0.1)],
        "foo",
        automatic=True,
    )


if __name__ == "__main__":
    main()
