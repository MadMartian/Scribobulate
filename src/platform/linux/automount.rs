//! Whether a directory is an automount point, read from this process's mount table.
//!
//! Linux's automounter (`autofs`, from the `autofs` daemon's maps or a systemd
//! `.automount` unit) mounts an `autofs` filesystem at each map's directory. Looking up a
//! name inside an INDIRECT map mounts what the name denotes: with `/net -hosts`,
//! `/net/<host>` asks `<host>` for its NFS exports. A DIRECT map's directory is itself
//! the trigger.
//!
//! **Read from `/proc/self/mountinfo`, never through the path.** `statfs` on the
//! directory would answer for an indirect map's root, but at a direct mount point that is
//! already mounted it reports the filesystem stacked on top, not `autofs` (MEASURED,
//! Ubuntu 22.04 / kernel 6.8: `stat -f` on `/proc/sys/fs/binfmt_misc` reads
//! `binfmt_misc` while the mount table lists an `autofs` mount at the same point). Whether
//! `statfs` on a direct point that is NOT yet mounted would mount it was not measured.
//! The mount table answers both cases without touching the path at all.
//!
//! **Read once, then again only when a mount changes.** The image and link gates ask
//! about every directory of every image path on every render, so a fresh read per
//! question had the kernel generate the whole table hundreds of times a render on a host
//! with many mounts. The table is kept, parsed down to its `autofs` mount points, and the
//! file stays open: the kernel marks an open mount table `POLLPRI` when this mount
//! namespace changes (`proc(5)`, `/proc/pid/mounts`), so one non-blocking `poll` per
//! question says whether the kept copy is still the table. MEASURED (kernel 6.8, in an
//! unprivileged `unshare -rm` namespace): a `tmpfs` mount raised `POLLPRI | POLLERR` on a
//! table opened before it, and the next `poll` was quiet again.
//!
//! **Unreadable fails closed for the stock maps.** Where `/proc` is hidden (a sandbox,
//! `hidepid`), nothing here can see the mounts. Rather than answer "no automount
//! anywhere", which lets the walk look up `/net/<host>` and contact the host, the
//! directories the `autofs` package's stock `auto.master` mounts ([`STOCK_MAP_DIRS`]) are
//! assumed to be automount points. A map configured elsewhere is not known then.
//!
//! Supplies the fact only. What to do about it is the image and link gates' decision
//! (`links::links_reach_foreign`).

use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Seek};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

const MOUNT_TABLE: &str = "/proc/self/mountinfo";

/// The directories the `autofs` package's stock `auto.master` serves (`/net -hosts`,
/// `/misc /etc/auto.misc`), taken as automount points while the mount table cannot be
/// read.
const STOCK_MAP_DIRS: &[&str] = &["/net", "/misc"];

/// Whether an `autofs` filesystem is mounted at exactly `dir`, so that looking up a name
/// inside it can mount something. While the mount table cannot be read, whether `dir`
/// is one of [`STOCK_MAP_DIRS`].
pub(crate) fn is_automount_directory(dir: &Path) -> bool {
    static KEPT: Mutex<Kept> = Mutex::new(Kept::new());
    let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    kept.refresh();
    kept.points.iter().any(|point| point == dir)
}

/// The mount table as last read, and the open file that says when it changes.
struct Kept {
    file: Option<File>,
    points: Vec<PathBuf>,
}

impl Kept {
    const fn new() -> Self {
        Kept {
            file: None,
            points: Vec::new(),
        }
    }

    /// Bring [`Kept::points`] up to date: read the table when nothing is kept or the
    /// kernel says it changed, else keep what was read.
    fn refresh(&mut self) {
        self.refresh_with(mounts_changed, || {
            let mut file = File::open(MOUNT_TABLE)?;
            let mut table = Vec::new();
            file.read_to_end(&mut table)?;
            // Rewound so a later re-read through the same file starts at the top.
            file.rewind()?;
            Ok((file, table))
        });
    }

    /// [`Kept::refresh`] over how to ask the kept file whether the table changed: asked
    /// only when a file is kept, and with none kept (never read, or unreadable last
    /// time) the table is read regardless.
    fn refresh_with(
        &mut self,
        changed: impl FnOnce(&File) -> bool,
        read: impl FnOnce() -> std::io::Result<(File, Vec<u8>)>,
    ) {
        let changed = self.file.as_ref().is_none_or(changed);
        self.update(changed, read);
    }

    /// [`Kept::refresh`]'s decision, over whether the table changed and how to read it.
    fn update(&mut self, changed: bool, read: impl FnOnce() -> std::io::Result<(File, Vec<u8>)>) {
        if !changed {
            return;
        }
        let was_readable = self.file.is_some() || self.points.is_empty();
        match read() {
            Ok((file, table)) => {
                self.points = autofs_points(&table);
                self.file = Some(file);
            }
            Err(e) => {
                if was_readable {
                    log::warn!(
                        "automount check: {MOUNT_TABLE} cannot be read ({e}); treating \
                         {STOCK_MAP_DIRS:?} as automount directories until it can"
                    );
                }
                self.points = STOCK_MAP_DIRS.iter().map(PathBuf::from).collect();
                self.file = None;
            }
        }
    }
}

/// Whether the kernel has marked the open mount table `file` changed since it was last
/// polled. A failed `poll` answers "changed", so the table is read again rather than
/// trusted stale.
fn mounts_changed(file: &File) -> bool {
    let mut fd = libc::pollfd {
        fd: file.as_raw_fd(),
        events: libc::POLLPRI,
        revents: 0,
    };
    // SAFETY: `fd` is one valid `pollfd` for the call to fill, and the descriptor it
    // names belongs to `file`, which outlives the call. A zero timeout never blocks.
    let ready = unsafe { libc::poll(&mut fd, 1, 0) };
    ready != 0 && (ready < 0 || fd.revents & (libc::POLLPRI | libc::POLLERR) != 0)
}

/// Every mount point the `mountinfo` text `table` lists with filesystem type `autofs`.
fn autofs_points(table: &[u8]) -> Vec<PathBuf> {
    table
        .split(|&b| b == b'\n')
        .filter_map(|line| {
            // `<id> <parent> <maj:min> <root> <mount point> <options> [optional...] - <fstype> ...`
            let mut fields = line.split(|&b| b == b' ');
            let point = fields.nth(4)?;
            let fstype = fields.skip_while(|f| *f != b"-").nth(1);
            (fstype == Some(b"autofs".as_slice()))
                .then(|| PathBuf::from(OsStr::from_bytes(&unescape(point))))
        })
        .collect()
}

/// Undo `mountinfo`'s octal escapes (`\040` space, `\011` tab, `\012` newline, `\134`
/// backslash), so a mount point with a space in it compares equal to its path.
fn unescape(field: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(field.len());
    let mut i = 0;
    while i < field.len() {
        let octal = field
            .get(i + 1..i + 4)
            .filter(|d| field[i] == b'\\' && d.iter().all(|c| (b'0'..=b'7').contains(c)));
        if let Some(d) = octal {
            out.push(
                d.iter()
                    .fold(0u8, |acc, c| acc.wrapping_mul(8) + (c - b'0')),
            );
            i += 4;
        } else {
            out.push(field[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{autofs_points, is_automount_directory, mounts_changed, Kept, STOCK_MAP_DIRS};
    use std::path::Path;

    fn autofs_at(table: &[u8], dir: &Path) -> bool {
        autofs_points(table).iter().any(|point| point == dir)
    }

    const TABLE: &[u8] = b"\
22 1 8:1 / / rw,relatime shared:1 - ext4 /dev/sda1 rw
39 26 0:34 / /proc/sys/fs/binfmt_misc rw,relatime shared:14 - autofs systemd-1 rw,direct
41 39 0:40 / /proc/sys/fs/binfmt_misc rw,relatime shared:20 - binfmt_misc binfmt_misc rw
50 22 0:74 / /net rw,relatime shared:9 - autofs /etc/auto.net rw,indirect
51 22 0:75 / /mnt/my\\040maps rw,relatime - autofs /etc/auto.x rw,indirect
";

    #[test]
    fn an_autofs_mount_point_is_an_automount_whatever_is_stacked_on_it() {
        assert!(autofs_at(TABLE, Path::new("/net")));
        assert!(
            autofs_at(TABLE, Path::new("/proc/sys/fs/binfmt_misc")),
            "a direct mount point is one even after its real filesystem is mounted on top"
        );
        assert!(
            autofs_at(TABLE, Path::new("/mnt/my maps")),
            "an escaped space in the mount point is decoded"
        );
    }

    #[test]
    fn other_mounts_and_directories_inside_a_map_are_not_automount_points() {
        assert!(!autofs_at(TABLE, Path::new("/")));
        assert!(!autofs_at(TABLE, Path::new("/net/host")));
        assert!(!autofs_at(TABLE, Path::new("/ne")));
        assert!(!autofs_at(TABLE, Path::new("/tmp")));
    }

    /// The table is read once and kept until the kernel reports a change; an
    /// unreadable table falls back to the stock maps rather than to "none".
    ///
    /// Mutations, each failing here: re-reading when unchanged; keeping the old points
    /// across a change; answering an unreadable table with no automount points.
    #[test]
    fn the_table_is_kept_until_it_changes_and_an_unreadable_one_fails_closed() {
        let file = || std::fs::File::open(std::env::current_exe().unwrap()).unwrap();
        let mut kept = Kept::new();
        kept.update(true, || Ok((file(), TABLE.to_vec())));
        assert!(kept.points.iter().any(|p| p == Path::new("/net")));
        kept.update(false, || panic!("an unchanged table is not read again"));
        assert!(kept.points.iter().any(|p| p == Path::new("/net")));
        kept.update(true, || {
            Ok((file(), b"22 1 8:1 / / rw - ext4 /dev/sda1 rw\n".to_vec()))
        });
        assert!(
            kept.points.is_empty(),
            "a changed table replaces the kept one"
        );
        kept.update(true, || Err(std::io::ErrorKind::PermissionDenied.into()));
        for dir in STOCK_MAP_DIRS {
            assert!(kept.points.iter().any(|p| p == Path::new(dir)), "{dir}");
        }
        assert!(
            kept.file.is_none(),
            "an unreadable table is tried again next time"
        );
    }

    /// The real kernel: a table just opened has no change pending, so a render's many
    /// questions cost one `poll` each, not one read each.
    #[test]
    fn a_freshly_opened_mount_table_reports_no_change() {
        let Ok(table) = std::fs::File::open(super::MOUNT_TABLE) else {
            println!("SKIPPED [TDD 2.7]: this host hides {}", super::MOUNT_TABLE);
            return;
        };
        assert!(!mounts_changed(&table));
    }

    /// `refresh` asks the kept file whether the table changed, and only a kept file: with
    /// none it reads without asking, and with one it reads exactly when told of a change.
    ///
    /// Mutations, each failing here: never asking (`changed` taken as `false`); always
    /// re-reading; asking with no file kept.
    #[test]
    fn refresh_reads_when_nothing_is_kept_or_the_kept_file_reports_a_change() {
        let file = || std::fs::File::open(std::env::current_exe().unwrap()).unwrap();
        let mut kept = Kept::new();
        kept.refresh_with(
            |_| panic!("with no file kept there is nothing to ask"),
            || Ok((file(), TABLE.to_vec())),
        );
        assert!(kept.points.iter().any(|p| p == Path::new("/net")));
        kept.refresh_with(|_| false, || panic!("an unchanged table is not read again"));
        let mut read = false;
        kept.refresh_with(
            |_| true,
            || {
                read = true;
                Ok((file(), Vec::new()))
            },
        );
        assert!(read, "a reported change reads the table again");
        assert!(kept.points.is_empty());
    }

    /// The real kernel's positive control: in a private mount namespace, a mount made
    /// after the table was opened is reported once, and the next question is quiet. A
    /// `mounts_changed` that never answers `true` fails here, where the cache would
    /// otherwise miss an `autofs` map mounted after start-up and answer "no automount".
    ///
    /// A forked child, because the namespace is entered with `unshare`, which a
    /// multi-threaded process cannot do and which must not touch the test process. The
    /// child only makes system calls on buffers built before the fork: allocating in a
    /// forked child of a multi-threaded process can deadlock on a `malloc` lock.
    #[test]
    fn a_mount_after_the_table_is_opened_is_reported_once() {
        use std::ffi::CString;
        use std::os::fd::FromRawFd;
        use std::os::unix::ffi::OsStrExt;
        // A host may refuse the namespace outright, or (Ubuntu 24.04's AppArmor
        // `apparmor_restrict_unprivileged_userns`) grant it without the capabilities
        // to map a user or mount in it; either way the kernel cannot be asked here.
        const NAMESPACE_REFUSED: i32 = 77;
        const NO_CHANGE_SEEN: i32 = 1;
        const CHANGE_STAYED: i32 = 2;
        const SETUP_FAILED: i32 = 3;
        let dir = tempfile::tempdir().unwrap();
        let target = CString::new(dir.path().as_os_str().as_bytes()).unwrap();
        let table = CString::new(super::MOUNT_TABLE).unwrap();
        let uid_map_path = CString::new("/proc/self/uid_map").unwrap();
        // SAFETY: `getuid` cannot fail.
        let uid_map = format!("0 {} 1", unsafe { libc::getuid() });
        let tmpfs = CString::new("tmpfs").unwrap();
        // SAFETY: the child calls only system calls, on the buffers built above, and
        // leaves with `_exit`; the parent reaps it.
        let child = unsafe { libc::fork() };
        assert!(child >= 0, "fork failed");
        if child == 0 {
            unsafe {
                if libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS) != 0 {
                    libc::_exit(NAMESPACE_REFUSED);
                }
                // Map this user to root in the namespace, so the temporary directory,
                // owned by this user, is one its capabilities reach.
                let map = libc::open(uid_map_path.as_ptr(), libc::O_WRONLY);
                let written = libc::write(map, uid_map.as_ptr().cast(), uid_map.len());
                libc::close(map);
                let fd = libc::open(table.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
                if written < 0 {
                    libc::_exit(NAMESPACE_REFUSED);
                }
                if fd < 0 {
                    libc::_exit(SETUP_FAILED);
                }
                let opened = std::fs::File::from_raw_fd(fd);
                if mounts_changed(&opened) {
                    libc::_exit(SETUP_FAILED);
                }
                let mounted = libc::mount(
                    tmpfs.as_ptr(),
                    target.as_ptr(),
                    tmpfs.as_ptr(),
                    0,
                    std::ptr::null(),
                );
                if mounted != 0 {
                    libc::_exit(if *libc::__errno_location() == libc::EPERM {
                        NAMESPACE_REFUSED
                    } else {
                        SETUP_FAILED
                    });
                }
                if !mounts_changed(&opened) {
                    libc::_exit(NO_CHANGE_SEEN);
                }
                if mounts_changed(&opened) {
                    libc::_exit(CHANGE_STAYED);
                }
                libc::_exit(0);
            }
        }
        let mut status = 0;
        // SAFETY: reaping the child forked above.
        unsafe { libc::waitpid(child, &mut status, 0) };
        assert!(libc::WIFEXITED(status), "child died: {status}");
        match libc::WEXITSTATUS(status) {
            0 => {}
            NAMESPACE_REFUSED => crate::testsymlink::skipped(
                "TDD 2.7 mount-table change detection",
                "this host refuses an unprivileged user namespace, or a mount in one",
            ),
            NO_CHANGE_SEEN => panic!("a mount after the table was opened was not reported"),
            CHANGE_STAYED => panic!("a reported change was reported again"),
            other => panic!("the namespace could not be set up (exit {other})"),
        }
    }

    #[test]
    fn an_ordinary_directory_is_not_an_automount() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_automount_directory(dir.path()));
    }

    /// Reads the real table, so a parser that never matches cannot pass by agreeing with
    /// fixtures written to suit it.
    #[test]
    fn this_hosts_own_autofs_mounts_are_found() {
        let table = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
        let Some(point) = table.lines().find_map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            let dash = f.iter().position(|x| *x == "-")?;
            (f.get(dash + 1) == Some(&"autofs") && !f[4].contains('\\')).then(|| f[4].to_owned())
        }) else {
            println!("SKIPPED [TDD 2.7]: this host has no autofs mount");
            return;
        };
        assert!(is_automount_directory(Path::new(&point)), "{point}");
    }
}
