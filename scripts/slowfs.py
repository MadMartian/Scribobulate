#!/usr/bin/env python3
"""A pass-through FUSE filesystem that answers slowly, for the manual-test checks
that need one.

Several `tests/MANUAL-TEST.md` items — 1.4c, 1.4d, 4.10, 4.11, 5.5, 5.7 — assert
what the application does *while the filesystem is slow to answer*: that the window
keeps redrawing, that a crash-recovery snapshot is not delayed behind document
reads, that a second Save is dropped rather than raced, and that a reload in flight
when a save lands does not revert the document. On a local disk every one of those
operations returns before anything could be observed, so the checks are unrunnable
and were being recorded as NOT RUN.

This mounts a real, **read-only** filesystem that sleeps for a configurable number
of milliseconds before each read, backed by an ordinary directory. It needs **no
root**: unprivileged FUSE via `fusermount`.

    scripts/slowfs.py --backing /tmp/docs --mount /tmp/slow --read-ms 1500
    # …drive the app against /tmp/slow…
    fusermount -u /tmp/slow

# Read-only, and why — do not "fix" this without checking

Delaying *writes* was implemented and removed. The distribution's `python3-fuse`
(1.0.2, the C extension) cannot serve writes on Python 3.10+ at all: its write path
uses a `#` format string in a module built without `PY_SSIZE_T_CLEAN`, so every
write fails with

    SystemError: PY_SSIZE_T_CLEAN macro must be defined for '#' formats

surfacing to the caller as `EIO` — or, via the naive reopen-per-call shape tried
first, as `EINVAL`. **Both look exactly like a defect in the application under
test**: the app dutifully reported "save failed: Invalid argument" against a rig
that could not accept a write. That is the expensive way for a harness to be wrong,
which is why it is written down here rather than rediscovered.

The mount therefore refuses write opens outright (`EROFS`) instead of accepting them
and failing obscurely — a clear refusal is a better harness than a confusing one.

# So which checks does this cover, and what covers the rest?

Covered here (reads only): **1.4c(a)** the window stays responsive while a document
is opened, **1.4d** a slow read does not delay a crash-recovery snapshot (the state
directory is deliberately not on this mount), **5.5** the admission check on re-read.

Not covered here (they need a slow *write*): **1.4c(b)**, **4.10**, **4.11**, **5.7**.
Those are pinned instead by `crate::docio::slow_io`, a `#[cfg(test)]`-only delay
injected on the pool thread — same condition, deterministic, every platform, and
absent from the shipped binary. To exercise them against a real filesystem, either
`pip install --user fusepy` (pure-Python ctypes binding, no C extension, no
`PY_SSIZE_T_CLEAN` problem) and port this file to it, or use sshfs to localhost and
`kill -STOP` the sshfs process for an indefinite hang.

Caveats worth knowing before trusting a run:

* The delay is per read **call**, not per file, so a large document read in many
  chunks is slowed many times. Keep fixtures small and the delay large.
* `getattr` is deliberately NOT delayed. The application stats a path before reading
  it (the `limits` admission check), and delaying that too would make every directory
  listing in the file chooser crawl without testing anything.
* This is a latency simulator, not a fault injector: it never fails an operation.
"""

import argparse
import errno
import os
import time

import fuse

fuse.fuse_python_api = (0, 2)


class SlowFS(fuse.Fuse):
    """Pass every operation through to `backing`, sleeping before data transfer."""

    def __init__(self, *args, **kwargs):
        self.backing = "/"
        self.read_ms = 0
        self.write_ms = 0
        super().__init__(*args, **kwargs)

    def _real(self, path):
        return os.path.join(self.backing, path.lstrip("/"))

    # ── metadata: never delayed, see the module doc ────────────────────────────
    def getattr(self, path):
        try:
            return os.lstat(self._real(path))
        except OSError as e:
            return -e.errno

    def readdir(self, path, offset):
        yield fuse.Direntry(".")
        yield fuse.Direntry("..")
        try:
            for name in os.listdir(self._real(path)):
                yield fuse.Direntry(name)
        except OSError:
            return

    def access(self, path, mode):
        if not os.access(self._real(path), mode):
            return -errno.EACCES

    def utime(self, path, times):
        os.utime(self._real(path), times)

    def truncate(self, path, size):
        with open(self._real(path), "r+b") as f:
            f.truncate(size)

    def unlink(self, path):
        os.unlink(self._real(path))

    def rename(self, old, new):
        os.rename(self._real(old), self._real(new))

    def mkdir(self, path, mode):
        os.mkdir(self._real(path), mode)

    def rmdir(self, path):
        os.rmdir(self._real(path))

    def chmod(self, path, mode):
        os.chmod(self._real(path), mode)

    def chown(self, path, uid, gid):
        os.chown(self._real(path), uid, gid)

    def mknod(self, path, mode, dev):
        os.mknod(self._real(path), mode, dev)

    # ── data transfer: this is where the delay lives ───────────────────────────
    def open(self, path, flags):
        if flags & (os.O_WRONLY | os.O_RDWR | os.O_CREAT):
            return -errno.EROFS
        try:
            os.close(os.open(self._real(path), flags))
        except OSError as e:
            return -e.errno

    def read(self, path, size, offset):
        time.sleep(self.read_ms / 1000.0)
        try:
            with open(self._real(path), "rb") as f:
                f.seek(offset)
                return f.read(size)
        except OSError as e:
            return -e.errno


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--backing", required=True, help="real directory holding the files")
    ap.add_argument("--mount", required=True, help="empty directory to mount over")
    ap.add_argument("--read-ms", type=int, default=1000)
    ap.add_argument(
        "--write-ms",
        type=int,
        default=0,
        help="accepted and IGNORED — see the module doc; the mount is read-only",
    )
    ap.add_argument(
        "--foreground", action="store_true", help="stay in the foreground (debugging)"
    )
    args = ap.parse_args()

    os.makedirs(args.backing, exist_ok=True)
    os.makedirs(args.mount, exist_ok=True)

    server = SlowFS(version="%prog " + fuse.__version__, dash_s_do="setsingle")
    server.backing = os.path.abspath(args.backing)
    server.read_ms = args.read_ms
    server.write_ms = args.write_ms
    # `-s` (single-threaded) is deliberate: it makes the delay the ONLY source of
    # ordering, so a check that depends on one operation being outstanding while
    # another starts is reproducible rather than dependent on FUSE's own threading.
    server.parse(
        ["-s", "-o", "default_permissions", args.mount]
        + (["-f"] if args.foreground else []),
        values=server,
        errex=1,
    )
    server.main()


if __name__ == "__main__":
    main()
