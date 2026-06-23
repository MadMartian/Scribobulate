//! macOS single-instance activation — the substitute for the D-Bus handoff
//! `GApplication` performs on Linux.
//!
//! GIO implements `GApplication` uniqueness over a **D-Bus session bus**. macOS
//! runs no session bus (and Homebrew's GLib ships no macOS-specific replacement
//! backend), so `g_application_register()` there finds no peer, every launch
//! elects itself primary, and TDD §8.1/8.2 fail silently: two processes, two
//! windows, two independent live-reload monitors on the same file. There is no
//! GIO switch to flip — the substitute has to be app-side.
//!
//! Shape follows `platform/win32/`, the Windows port's precedent for an
//! OS-integration module: one file, `#[cfg]`-gated at the module *declaration*
//! and never internally, hand-rolled FFI rather than a new binding crate, and it
//! does **only** the OS-specific handoff — the forwarded arguments are fed into
//! the existing `GApplication` `open`/`activate` handlers (`app/setup.rs`), which
//! this module never reimplements or second-guesses.
//!
//! Primitive: an `flock`ed lock file elects the primary; a Unix domain socket
//! beside it carries forwarded arguments to the winner. Both live inside a
//! `0700`, owner-only directory under `$TMPDIR` (see [`ensure_private_dir`] for
//! why a bare shared directory is not a safe rendezvous point — that used to be
//! the design here, and QA round R1 (S01/S02) is why it no longer is). The
//! kernel releases the lock when the primary exits *however* it exits, so a
//! crashed instance cannot wedge subsequent launches — the next primary unlinks
//! the stale socket once it holds the lock.

use gtk::gio;
use gtk::prelude::*;
use gtk::{glib, Application};
use std::fs::File;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

/// Wire-format tag. A version suffix is cheap now and the only thing that lets a
/// future format change fail loudly instead of misreading a mixed-version pair
/// of builds (which, sharing a `$TMPDIR`, genuinely can meet).
const PROTOCOL_TAG: &str = "scribobulate-open-v1";

/// Hard cap on one forwarded message. Generous for any real command line —
/// room for well over a thousand `file://` URIs — while turning
/// `DataInputStream::read_line`'s uncapped internal buffer growth into a
/// small, logged, contained failure instead of unbounded heap growth (QA R1,
/// finding R1-S01, sub-point (c): a connected peer that never sends a
/// newline could otherwise grow this process's memory without bound).
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// The outcome of the election. `main` keeps the [`Guard`] alive for the
/// process's lifetime and exits immediately on [`Launch::Forwarded`].
pub(crate) enum Launch {
    /// This process owns the instance and must run normally.
    Primary(Guard),
    /// The arguments were handed to the already-running primary. Exit.
    Forwarded,
}

/// Holds the primary's claim. Dropping it releases the lock and unlinks the
/// socket; every field is optional because a failure to establish the handoff
/// degrades to *exactly today's behaviour* (an independent process) rather than
/// refusing to launch — a broken `$TMPDIR` must not make the app unstartable.
#[derive(Default)]
pub(crate) struct Guard {
    lock: Option<File>,
    service: Option<gio::SocketService>,
    socket: Option<PathBuf>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(service) = self.service.take() {
            service.stop();
        }
        if let Some(path) = self.socket.take() {
            let _ = std::fs::remove_file(path);
        }
        // `lock` closes with the struct; the kernel drops the flock with it.
        // The rendezvous directory itself, and the now-unlocked lock file
        // inside it, are deliberately left behind for the next launch to
        // reuse via `ensure_private_dir`'s "already exists" branch.
        self.lock.take();
    }
}

/// Elect this process primary, or forward `args` to the running primary.
///
/// `args` is argv minus the program name and minus the already-stripped
/// `--new-instance` switch — i.e. exactly the list `run_with_args` would hand to
/// `open`.
pub(crate) fn elect(app: &Application, args: &[String]) -> Launch {
    let Some((lock_path, sock_path)) = paths() else {
        return Launch::Primary(Guard::default());
    };

    let lock = match File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            log::warn!("single-instance: cannot open {}: {e}", lock_path.display());
            return Launch::Primary(Guard::default());
        }
    };

    match lock.try_lock() {
        Ok(()) => become_primary(app, lock, sock_path),
        Err(std::fs::TryLockError::WouldBlock) => {
            if forward(&sock_path, args) {
                log::info!(
                    "single-instance: handed {} argument(s) to the running instance",
                    args.len()
                );
                return Launch::Forwarded;
            }
            // The lock is held but nothing answered. Either the primary is
            // between taking the lock and binding the socket (the retry inside
            // `forward` already covers that), or it is shutting down. Launching
            // independently is the honest fallback: refusing to start would turn
            // a transient race into a user-visible failure to open a document.
            log::warn!(
                "single-instance: a peer holds {} but is not answering {}; launching independently",
                lock_path.display(),
                sock_path.display()
            );
            Launch::Primary(Guard::default())
        }
        Err(e) => {
            log::warn!("single-instance: cannot lock {}: {e}", lock_path.display());
            Launch::Primary(Guard::default())
        }
    }
}

/// Where the lock file and socket live.
///
/// Read from `std::env`, deliberately **not** through GLib's user-directory
/// helpers. Both processes in a handoff must derive the *same* path from their
/// own environments, and GLib's user dirs are XDG-derived — the same coupling
/// `workaround.rs` already redirects `XDG_CONFIG_HOME` through, and that
/// `clippy.toml` bans `glib::user_config_dir` over. That redirect is inert here
/// (it is gated on GTK < 4.12, and Homebrew ships 4.22), but an agreement
/// protocol whose rendezvous point is only stable *because a defence upstream
/// of it happens not to fire* is a bad bargain: a mismatch would not fail, it
/// would silently return two primaries — the exact defect this module removes.
/// `$TMPDIR` has no such coupling, and on macOS it is already per-user
/// (`/var/folders/<hash>/T/`), which `open`-launched bundles and terminal
/// launches are confirmed to share.
///
/// The lock and socket themselves live one level down, inside a private
/// owner-only directory — see [`ensure_private_dir`] for why a bare shared
/// directory (which is all `$TMPDIR` guarantees, and *all* the `/tmp`
/// fallback below it guarantees) is not a safe rendezvous point on its own.
///
/// Returns `None` when no usable path exists, which callers treat as "no
/// single-instance handoff" rather than an error.
fn paths() -> Option<(PathBuf, PathBuf)> {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    // The uid keeps the directory name per-user under the `/tmp` fallback;
    // under a real `$TMPDIR` it is merely redundant.
    let uid = unsafe { libc::getuid() };
    let dir = base.join(format!("scribobulate-{uid}"));

    if !ensure_private_dir(&dir) {
        return None;
    }

    let sock = dir.join("handoff.sock");
    let lock = dir.join("handoff.lock");

    // `sockaddr_un.sun_path` is 104 bytes on macOS (`sys/un.h`) and an
    // over-long path is TRUNCATED, not rejected — two processes would then bind
    // and connect to different-but-equally-truncated names with no error
    // anywhere. Refuse the handoff instead of shipping a silent mismatch.
    if sock.as_os_str().len() >= 100 {
        log::warn!(
            "single-instance: {} exceeds the macOS socket-path limit; handoff disabled",
            sock.display()
        );
        return None;
    }
    Some((lock, sock))
}

/// Create, or validate a pre-existing, directory that only its owner can
/// enter — the actual access-control boundary for the whole handoff.
///
/// QA round R1, finding R1-S01: a bare shared temp directory (`/tmp` when
/// `$TMPDIR` is unset, and even a real per-user `$TMPDIR` is still just "not
/// world-writable", not "world-unreadable") lets a local attacker pre-create
/// the lock/socket filenames themselves, at a mode of their own choosing, and
/// either hijack a forwarded document path or wedge every future launch by
/// holding the lock. Putting both files one level down, inside a directory
/// only `getuid()` can *traverse into*, removes that class of attack
/// entirely: path resolution requires search (`x`) permission on every
/// parent path component, so a `0700` directory makes everything inside it
/// unreachable to every other local user — regardless of what mode ends up on
/// the files themselves. This is also why R1-S02's socket-narrowing race is
/// no longer the primary defence: it still happens (below, in
/// `become_primary`, belt-and-braces), but the directory is what actually
/// keeps other users out during the window before that narrowing runs.
///
/// A pre-existing entry is trusted only if it is *exactly* ours: a real
/// directory — `symlink_metadata` does not follow links, so a symlink here
/// reports its own type and fails `is_dir()`, never the target's — owned by
/// us, with no group/other permission bits at all. Anything else is refused
/// rather than reused: repairing a suspicious directory in place would just
/// be R1-S02's original mistake (best-effort narrowing after the fact) one
/// level up.
///
/// `/tmp` itself carries the sticky bit on macOS, so once this directory is
/// created no other local user can unlink or replace it out from under a
/// later check even though `/tmp` is world-writable — only its owner (or
/// root) can remove an entry they don't own from a sticky directory.
fn ensure_private_dir(dir: &Path) -> bool {
    let uid = unsafe { libc::getuid() };
    match std::fs::symlink_metadata(dir) {
        Ok(meta) => {
            let is_dir = meta.file_type().is_dir();
            let owner_uid = meta.uid();
            let mode = meta.mode();
            if is_dir && owner_uid == uid && mode & 0o077 == 0 {
                true
            } else {
                log::warn!(
                    "single-instance: refusing pre-existing {} (is_dir={is_dir}, uid={owner_uid}, expected {uid}, mode={mode:#o}); handoff disabled",
                    dir.display()
                );
                false
            }
        }
        Err(_) => {
            // `DirBuilder::mode` can only be narrowed by the process umask,
            // never widened by it — umask clears requested bits, it never
            // sets ones we didn't ask for — so a `0700` request can never
            // produce a directory with group/other bits set, regardless of
            // the ambient umask. No follow-up `chmod` is needed for
            // correctness; unlike R1-S02's socket narrowing, there is no
            // window here where the directory exists at a looser mode.
            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700);
            match builder.create(dir) {
                Ok(()) => true,
                Err(e) => {
                    log::warn!("single-instance: cannot create {}: {e}", dir.display());
                    false
                }
            }
        }
    }
}

// ── primary side ─────────────────────────────────────────────────────────────

fn become_primary(app: &Application, lock: File, sock_path: PathBuf) -> Launch {
    // We hold the lock, so no live peer can own this path: anything here is the
    // corpse of a killed primary.
    let _ = std::fs::remove_file(&sock_path);

    let service = gio::SocketService::new();
    let address = gio::UnixSocketAddress::new(&sock_path);
    if let Err(e) = service.add_address(
        &address,
        gio::SocketType::Stream,
        gio::SocketProtocol::Default,
        None::<&glib::Object>,
    ) {
        log::warn!(
            "single-instance: cannot listen on {}: {e}",
            sock_path.display()
        );
        return Launch::Primary(Guard {
            lock: Some(lock),
            service: None,
            socket: None,
        });
    }
    // Belt-and-braces alongside `ensure_private_dir`: that directory is the
    // actual access-control boundary (nobody else can even resolve a path
    // into it), but narrowing the socket's own mode costs nothing and means
    // it is never world-connectable even if it somehow ends up somewhere
    // less private than expected. QA finding R1-S02 was that the previous
    // version of this code discarded a failure here (`let _ = ...`), so the
    // socket could stay world-connectable permanently with nothing logged.
    // This version treats that failure the same as a failed `add_address`
    // above: tear the listener down and fall back to an independent launch
    // rather than serve on a socket whose permissions could not be confirmed.
    if let Err(e) = std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o600)) {
        log::warn!(
            "single-instance: cannot narrow {} to owner-only: {e}; launching independently",
            sock_path.display()
        );
        service.stop();
        let _ = std::fs::remove_file(&sock_path);
        return Launch::Primary(Guard {
            lock: Some(lock),
            service: None,
            socket: None,
        });
    }

    service.connect_incoming(glib::clone!(
        #[weak]
        app,
        #[upgrade_or]
        true,
        move |_service, connection, _| {
            // GSocketService drops its reference as soon as this handler
            // returns, so the connection has to be owned by the async read.
            let connection = connection.clone();

            // Belt-and-braces alongside the owner-only rendezvous directory:
            // confirming the credential the KERNEL attaches to this specific
            // connection does not depend on the directory still being what we
            // think it is by the time this fires, and costs one syscall.
            if !peer_is_us(&connection) {
                log::warn!("single-instance: rejecting a connection from a different local user");
                let _ = connection.close(gio::Cancellable::NONE);
                return true;
            }

            glib::MainContext::default().spawn_local(async move {
                match read_bounded_line(connection.input_stream()).await {
                    Ok(Some(line)) => dispatch(&app, &line),
                    Ok(None) => log::debug!("single-instance: peer closed without sending"),
                    Err(e) => log::warn!("single-instance: read failed: {e}"),
                }
                let _ = connection.close(gio::Cancellable::NONE);
            });
            // Handled; no other handler should see this connection.
            true
        }
    ));
    service.start();
    log::info!(
        "single-instance: primary, listening on {}",
        sock_path.display()
    );

    Launch::Primary(Guard {
        lock: Some(lock),
        service: Some(service),
        socket: Some(sock_path),
    })
}

/// Whether the peer at the far end of `connection` is running as this same
/// local user.
///
/// `getpeereid` reads the credential the *kernel* attached to the connection
/// at `connect()` time — a peer cannot spoof it by sending anything on the
/// wire. QA finding R1-S01 asked for exactly this ("no LOCAL_PEERCRED, no
/// getpeereid, no token beyond `PROTOCOL_TAG`, which is a public constant").
fn peer_is_us(connection: &gio::SocketConnection) -> bool {
    let fd = connection.socket().as_raw_fd();
    let mut euid: libc::uid_t = 0;
    let mut egid: libc::gid_t = 0;
    // SAFETY: `fd` is the live, connected AF_UNIX socket backing `connection`
    // for the duration of this call, and the two out-parameters are valid,
    // correctly-typed stack locations for `getpeereid` to write through.
    let ok = unsafe { libc::getpeereid(fd, &mut euid, &mut egid) } == 0;
    ok && euid == unsafe { libc::getuid() }
}

/// Read up to a newline or [`MAX_MESSAGE_BYTES`], whichever comes first.
///
/// `DataInputStream::read_line` grows its internal buffer without a
/// documented size cap while it searches for the delimiter, so it is not
/// safe to hand a peer that might never send one (QA finding R1-S01,
/// sub-point (c)). This reads fixed-size chunks off the raw stream instead
/// and stops the moment the budget is exhausted, returning an error the
/// caller logs and treats as a dropped connection — never partial data.
async fn read_bounded_line(stream: gio::InputStream) -> Result<Option<Vec<u8>>, glib::Error> {
    let mut buf = Vec::new();
    loop {
        let chunk = stream
            .read_bytes_future(4096, glib::Priority::DEFAULT)
            .await?;
        if chunk.is_empty() {
            // Zero-length read is GIO's EOF signal.
            return Ok(if buf.is_empty() { None } else { Some(buf) });
        }
        for &byte in chunk.as_ref() {
            if byte == b'\n' {
                return Ok(Some(buf));
            }
            buf.push(byte);
            if buf.len() > MAX_MESSAGE_BYTES {
                return Err(glib::Error::new(
                    gio::IOErrorEnum::InvalidData,
                    "forwarded message exceeded the length limit",
                ));
            }
        }
    }
}

/// Turn one forwarded line into the same `open`/`activate` emission GIO's D-Bus
/// path produces on Linux, so both platforms converge on `app/setup.rs`'s
/// handlers with identical inputs.
fn dispatch(app: &Application, line: &[u8]) {
    let Ok(text) = std::str::from_utf8(line) else {
        log::warn!("single-instance: ignoring non-UTF-8 message");
        return;
    };
    let text = text.trim_end_matches(['\r', '\n']);
    let Some(payload) = text.strip_prefix(PROTOCOL_TAG) else {
        log::warn!("single-instance: ignoring message with an unrecognised protocol tag");
        return;
    };

    let mut files = Vec::new();
    let mut rejected = 0usize;
    let mut total = 0usize;
    for uri in payload.split_whitespace() {
        total += 1;
        // Only local files. The peer is same-uid-verified (`peer_is_us`) and
        // reachable only through the owner-only rendezvous directory, but
        // `open` would otherwise happily be pointed at a remote URI — the
        // same containment reflex `links.rs` applies to document resources,
        // applied to the process's other input surface.
        if is_local_file_uri(uri) {
            files.push(gio::File::for_uri(uri));
        } else {
            rejected += 1;
            log::warn!("single-instance: ignoring non-local-file URI in forwarded arguments");
        }
    }

    if total == 0 {
        // A genuinely bare re-launch: no arguments were forwarded at all.
        // `on_activate` already distinguishes this from a first activation
        // by "windows exist", so it opens a fresh document rather than
        // replaying the saved session.
        app.activate();
    } else if files.is_empty() {
        // QA finding R1-17: every forwarded argument was rejected. That is a
        // malformed (or hostile) message, not "no arguments" — falling
        // through to `activate()` here would let a peer supplying only
        // garbage URIs pop a blank window as if the user had asked for one.
        log::warn!(
            "single-instance: forwarded message carried {rejected} URI(s), all rejected; ignoring"
        );
    } else {
        // The empty hint is the non-interactive one: a forwarded command line
        // gets TDD 1.5/1.6's batch rule, not File ▸ Open's active-window rule.
        app.open(&files, "");
    }
}

/// Whether `uri` is a `file://` URI with no remote host component.
///
/// QA finding R1-S04: the previous check was `uri.starts_with("file://")`, a
/// prefix test that also accepts `file://some-host/share/x` — a URI naming a
/// REMOTE host, which `gio::File::for_uri` would then happily resolve as
/// such. A real scheme parse costs nothing extra and is the honest way to
/// express "local files only": accept an absent host (`file:///path`, the
/// normal form) or an explicit `localhost`, reject everything else.
fn is_local_file_uri(uri: &str) -> bool {
    let Ok(parsed) = glib::Uri::parse(uri, glib::UriFlags::NONE) else {
        return false;
    };
    if !parsed.scheme().eq_ignore_ascii_case("file") {
        return false;
    }
    matches!(
        parsed.host().as_deref(),
        None | Some("") | Some("localhost")
    )
}

// ── secondary side ───────────────────────────────────────────────────────────

/// Hand `args` to the running primary. Returns `false` if nothing answered.
fn forward(sock_path: &Path, args: &[String]) -> bool {
    let mut line = String::from(PROTOCOL_TAG);
    for arg in args {
        // `for_commandline_arg` resolves a relative path against THIS process's
        // cwd — the user's shell, which is the correct anchor and the reason the
        // conversion happens here rather than in the primary (whose cwd is
        // unrelated). It is also exactly what GIO's own D-Bus forwarding does,
        // so a macOS handoff and a Linux one deliver the same URIs. Percent
        // encoding makes the space-separated wire format safe for paths
        // containing spaces or newlines.
        line.push(' ');
        line.push_str(&gio::File::for_commandline_arg(arg).uri());
    }
    line.push('\n');

    // The primary takes the lock a few instructions before it binds the socket,
    // so a launch landing inside that window sees "locked but nothing
    // listening". The bound is wall-clock because the thing being waited on is
    // wall-clock (ScrAP-134's shape) — there is no main loop running in this
    // process to count frames against, and the whole budget is well under the
    // time a second window would take to appear.
    let client = gio::SocketClient::new();
    let address = gio::UnixSocketAddress::new(sock_path);
    for attempt in 0..6 {
        // Disambiguated: `ObjectExt::connect` (signal wiring) is in scope too,
        // and the two have compatible-looking arities.
        match gio::prelude::SocketClientExt::connect(&client, &address, gio::Cancellable::NONE) {
            Ok(connection) => {
                if let Err(e) = connection
                    .output_stream()
                    .write_all(line.as_bytes(), gio::Cancellable::NONE)
                {
                    log::warn!("single-instance: write to the primary failed: {e}");
                    return false;
                }
                let _ = connection.close(gio::Cancellable::NONE);
                return true;
            }
            Err(e) => {
                log::debug!("single-instance: connect attempt {attempt} failed: {e}");
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    false
}
