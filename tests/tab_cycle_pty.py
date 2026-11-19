#!/usr/bin/env python3
"""Exercise selected-completion previews in a real Bash PTY, without Docker.

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
        self, library, directory, bash_path, *, automatic=False, inline=False, preview=None,
        delay=0, mouse=False, files=(), setup=""
    ):
        self.output = bytearray()
        self.queries = {b"\x1b[>c": 0, b"\x1b[6n": 0}
        self.requests = directory / "requests"
        rcfile = directory / "bashrc"
        for filename in files:
            (directory / filename).touch()
        completion_setup = "" if files else "complete -o nospace -F __complete_cycle __cycle\n"
        rcfile.write_text(
            "PS1='$ '\n"
            "HISTFILE=/dev/null\n"
            "__cycle() { printf '\\nCYCLE_RESULT:<%s>\\n' \"$*\"; }\n"
            "__complete_cycle() {\n"
            f"  printf x >> {shlex.quote(str(self.requests))}\n"
            f"  sleep {delay}\n"
            "  COMPREPLY=()\n"
            "  local value\n"
            "  for value in far foo; do\n"
            '    [[ $value == "${COMP_WORDS[COMP_CWORD]}"* ]] && COMPREPLY+=("$value")\n'
            "  done\n"
            "  return 0\n"
            "}\n"
            f"{completion_setup}"
            f"enable -f {shlex.quote(str(library))} flyline || exit 1\n"
            "flyline --set-delayed-startup-ms 0\n"
            "flyline --show-animations false\n"
            f"flyline mouse --mode {'simple' if mouse else 'disabled'}\n"
            "flyline suggestions --sort-order alphabetical "
            f"--auto-suggest {str(automatic).lower()} "
            f"--auto-suggest-inline {str(inline).lower()}\n"
            "flyline editor --show-inline-history false\n"
            + (f"flyline --preview {str(preview).lower()}\n" if preview is not None else "")
            + "flyline key bind Ctrl+g 'always=submitOrNewline'\n"
            "flyline key bind Ctrl+y 'tabCompletionAvailable=tabCompletionAcceptAll'\n"
            "flyline key bind Ctrl+t 'tabCompletionEntrySelected=tabCompletionAcceptEntry'\n"
            + setup,
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
    library, name, steps, expected, *, bash_path, request_count=None,
    italic_after=None, cursor_after=None, no_result_after=(), menu_marker=b"CommandComp", **options
):
    with tempfile.TemporaryDirectory(prefix="flyline-cycle-test-") as directory:
        shell = Shell(library, Path(directory), bash_path, **options)
        try:
            deadline = time.monotonic() + 3
            while b"\x1b[?2004h" not in shell.output and time.monotonic() < deadline:
                shell.pump(0.02)
            assert b"\x1b[?2004h" in shell.output, "Flyline did not initialize the terminal"
            shell.pump(0.7)
            for index, (keys, wait) in enumerate(steps):
                output_start = len(shell.output)
                shell.send(keys, wait)
                if b"\t" in keys and not options.get("delay"):
                    # Wait for the actual completion menu, not a fixed delay:
                    # concurrent builds can leave the first request in flight.
                    deadline = time.monotonic() + 3
                    while menu_marker not in shell.output and time.monotonic() < deadline:
                        shell.pump(0.05)
                    assert menu_marker in shell.output, "Completion menu did not appear"
                if index == italic_after:
                    codes = re.findall(rb"\x1b\[([0-9;]*)m", shell.output[output_start:])
                    italic = any(b"3" in code.split(b";") for code in codes)
                    assert italic == (options.get("preview") is not False), (
                        "Selected completion preview did not honor the italic setting"
                    )
                if cursor_after and index in cursor_after:
                    # Flyline restores the terminal cursor after drawing each
                    # frame. CHA columns are one-based; PS1 is the fixed "$ ".
                    columns = re.findall(rb"\x1b\[(\d+)G", shell.output)
                    expected_column = cursor_after[index]
                    assert columns and int(columns[-1]) == expected_column, (
                        f"After step {index}: expected cursor column {expected_column}, got {columns[-1:]!r}"
                    )
                if index in no_result_after:
                    assert not re.search(rb"\r?\nCYCLE_RESULT:<", shell.output), (
                        "Completion was executed before the second Enter press"
                    )
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
        "Tab cycling, one Enter accepts and executes",
        [(b"__cycle f", 0.15), (b"\t", 0.5), (b"\t", 0.1),
         (b"\r", 0.1)], "foo", italic_after=1, cursor_after={1: 12, 2: 12},
    )
    run(
        "Escape preserves original input",
        [(b"__cycle f\t", 0.5), (b"\t", 0.1), (ESCAPE, 0.15), (b"\x07", 0.1)], "f",
    )
    run(
        "typing refines original input",
        [(b"__cycle f\t", 0.5), (b"o", 0.15), (ESCAPE, 0.1), (b"\x07", 0.1)],
        "fo", cursor_after={0: 12, 1: 13},
    )
    run(
        "accept all uses original replacement span",
        [(b"__cycle f\t", 0.5), (b"\x19", 0.1), (b"\x07", 0.1)], "far foo",
    )
    run(
        "arrow navigation then one Enter executes",
        [(b"__cycle f\t", 0.5), (b"\x1b[B", 0.1), (b"\r", 0.1)], "foo",
    )
    run(
        "Shift Tab wraps backwards then one Enter executes",
        [(b"__cycle f\t", 0.5), (b"\x1b[Z", 0.1), (b"\r", 0.1)], "foo",
    )
    run(
        "unselected background becomes selected on Tab",
        [(b"\x1b[200~__cycle f\x1b[201~", 0.1), (b"\x1b[C", 0.5), (b"\t", 0.15), (b"\t", 0.1),
         (b"\r", 0.1)], "foo", inline=True,
    )
    run(
        "automatic menu cancellation",
        [(b"\x1b[200~__cycle f\x1b[201~", 0.1), (b"\x1b[C", 0.5), (b"\x1b[B", 0.1), (ESCAPE, 0.15), (b"\x07", 0.1)],
        "f", automatic=True,
    )
    run(
        "Escape cancels pending completion",
        [(b"__cycle f\t", 0.05), (ESCAPE, 0.5), (b"\x07", 0.1)], "f", delay=0.35,
    )
    run(
        "typing cancels pending preview",
        [(b"__cycle f\t", 0.05), (b"b", 0.5), (b"\x07", 0.1)], "fb", delay=0.35,
    )
    for enter in [b"\r", b"\n"]:
        run(
            f"disabled preview accepts without execution until the second Enter ({enter!r})",
            [(b"__cycle f\t", 0.5), (b"\t", 0.1), (enter, 0.3), (enter, 0.1)],
            "foo", preview=False, italic_after=0, no_result_after={2},
        )
    run(
        "explicitly enabled preview accepts and executes with one Enter",
        [(b"__cycle f\t", 0.5), (b"\t", 0.1), (b"\r", 0.1)], "foo", preview=True,
    )
    run(
        "preview preserves trailing arguments",
        [(b"__cycle f tail", 0.15), (b"\x1b[D" * 5, 0.1), (b"\t", 0.5),
         (b"\t", 0.1), (b"\r", 0.1)], "foo tail",
    )
    run(
        "Ctrl J also accepts and executes in one press",
        [(b"__cycle f\t", 0.5), (b"\t", 0.1), (b"\n", 0.1)], "foo",
    )
    run(
        "unselected automatic suggestion is not accepted by Enter",
        [(b"\x1b[200~__cycle f\x1b[201~", 0.1), (b"\x1b[C", 0.5), (b"\r", 0.1)],
        "f", automatic=True,
    )
    run(
        "selected automatic suggestion executes with one Enter",
        [(b"\x1b[200~__cycle f\x1b[201~", 0.1), (b"\x1b[C", 0.5),
         (b"\x1b[B", 0.1), (b"\r", 0.1)], "far", automatic=True,
    )
    for accept in [b"\x1b[C", b"\x1b[F"]:
        run(
            f"selected preview accepted by {accept!r} survives Escape",
            [(b"\x1b[200~__cycle f\x1b[201~", 0.1), (b"\x1b[C", 0.6),
             (b"\x1b[B", 0.2), (b"\x1b[B", 0.2), (accept, 0.2),
             (ESCAPE, 0.1), (b"\x07", 0.1)],
            "foo", automatic=True, inline=True,
        )
    run(
        "hidden inline suggestion cannot be accepted word by word",
        [(b"\x1b[200~__cycle f\x1b[201~", 0.1), (b"\x1b[C", 0.6),
         (b"\x1b[B", 0.2), (b"\x1b[B", 0.2), (b"\x1b[1;5C", 0.2),
         (ESCAPE, 0.1), (b"\x07", 0.1)], "f", automatic=True, inline=True,
    )
    run(
        "disabling preview preserves original inline acceptance",
        [(b"\x1b[200~__cycle f\x1b[201~", 0.1), (b"\x1b[C", 0.6),
         (b"\x1b[B", 0.2), (b"\x1b[B", 0.2), (b"\x1b[C", 0.2), (b"\r", 0.1)],
        "far", automatic=True, inline=True, preview=False,
    )
    for preview in [True, False]:
        run(
            f"mouse click on typed letter retains its real position (preview={preview})",
            [(b"__cycle f\t", 0.6), (b"\x1b[<0;11;1M", 0.15),
             (b"\x1b[<0;11;1m", 0.15), (b"X", 0.2), (ESCAPE, 0.1), (b"\x07", 0.1)],
            "Xf", mouse=True, preview=preview,
        )
    for typed, browse, expected in [("i\u0307", b"\t", "İabc"), ("İ", b"", "i\u0307xyz")]:
        run(
            f"case-expanding Unicode prefix keeps cursor at typed text ({typed!r})",
            [(f"__cycle {typed}\t".encode(), 0.6), (browse, 0.2), (b"\r", 0.1)],
            expected, cursor_after={0: 12, 1: 12},
            setup=(
                "flyline suggestions set-fuzzy-mode none\n"
                "__complete_cycle() { COMPREPLY=('İabc' 'i\u0307xyz'); }\n"
            ),
        )
    run(
        "escaped filename space retains the cursor and executes the selected file",
        [(b"\x1b[200~__cycle foo\\ \x1b[201~", 0.6), (b"\t", 0.6), (b"\r", 0.1)],
        "foo one", files=("foo one", "foo two"), menu_marker=b"FilenameExpansion",
        cursor_after={0: 16, 1: 16},
    )
    run(
        "incomplete command continues on the next line after accepting",
        [(b"if __cycle f\t", 0.5), (b"\r", 0.1), (b"then :; fi\r", 0.1)], "far",
    )
    run(
        "explicit accept without execution is one undo step",
        [(b"__cycle f\t", 0.5), (b"\t", 0.1), (b"\x14", 0.1),
         (b"\x1a", 0.1), (b"\x07", 0.1)], "f",
    )


if __name__ == "__main__":
    main()
