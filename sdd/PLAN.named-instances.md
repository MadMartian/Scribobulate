# Plan: named instances

## Problem

The operator wants to run more than one long-lived copy of Scribobulate. Each copy should
keep its own windows, session and crash recovery, and each should be reusable: launching
"the work copy" again should hand the file to the running work copy, not start a third
process. A Linux `.desktop` launcher should open each copy.

Today there is exactly one reusable copy. `--new-instance` (`-n`, TDD 8.5) starts a
process that never reuses anything and is never reused. Redirecting `XDG_STATE_HOME` or
`XDG_CONFIG_HOME` does not separate copies either: the launch still finds the running
process and hands it the file.

### Root cause

Reuse is decided by the application ID, not by any directory. GApplication registers the
ID `com.extollit.scribobulate` on the session bus (D-Bus on Linux and Windows; on macOS the
`$TMPDIR` rendezvous in `src/platform/mac/single_instance.rs` plays that part). Any launch
that finds that ID taken forwards its arguments and exits, whatever its environment.
`-n` only opts out of the check (`NON_UNIQUE`), so it cannot give a reusable second copy.
A second reusable copy needs a second ID.

## Possible approaches

### ✅ 1. `--instance NAME`: the name selects the ID and the state subdirectory

An optional switch names the copy. It does two things:

- **ID:** `com.extollit.scribobulate.NAME`. Launches with the same name reuse each other.
  Launches with different names, or with no name, never see each other.
- **State:** the instance keeps its session, crash recovery and crash reports in
  `<state home>/scribobulate/instances/NAME/`. `<state home>` is `XDG_STATE_HOME` if set,
  otherwise the platform default, so by default every instance lives under the one home,
  side by side. The default instance keeps `<state home>/scribobulate/` itself; `instances/`
  only holds named instances and appears when the first one runs:

  ```
  <state home>/scribobulate/        default instance (unchanged)
  ├── session.toml, swap/, logs
  └── instances/
      └── work/                     --instance work
  ```

`XDG_STATE_HOME` and `XDG_CONFIG_HOME` keep their current meaning, so a launcher can also
move an instance's state, or give it its own settings, without any new switch.
Settings (`config.toml`, `themes.toml`) stay shared unless `XDG_CONFIG_HOME` is
redirected: they describe the reader's preferences, not the copy.

A launch without `--instance` is exactly today's app: same ID, same state paths. Nothing
existing moves.

**Pros**: explicit and predictable. The launcher can name both the ID and the window
class, so KDE, `StartupWMClass` and the Wayland `app_id` all match. No migration.
**Cons**: one more switch to parse before registration (GTK4Rs/AP-17), and three places
that key on the ID or the state directory must learn about the name.

### ❌ 2. Derive the ID from the XDG directories

Hash the resolved state and config directories into an ID suffix. Rejected: the ID is
unpredictable, so a `.desktop` file cannot name its window class. It also silently
splits every launch that redirects a directory, including `cargo run`, which pins
`XDG_CONFIG_HOME`.

### ❌ 3. `-n` plus redirected XDG directories

Rejected: `-n` turns reuse off. Every launch from such a launcher starts a new process.

### ❌ 4. An environment variable instead of a switch

Rejected: it leaks into every process the app starts (an external editor, a browser
opened from a link) and is invisible in `ps`. The operator asked for a switch.

### ❌ 5. Instance state beside the default (`scribobulate-NAME/`)

Rejected for `scribobulate/instances/NAME/`: a subdirectory keeps one tree to back up or
delete, inherits the state directory's owner-only mode from the one creator
(`session::create_state_dir`), and cannot collide with a future sibling directory.

## Recommendation

Approach 1.

**Name rules.** 1 to 32 characters, ASCII letters and digits only, and the first character
a letter. A D-Bus name element may not start with a digit. Names are case-insensitive and
stored lowercased, so `Work` and `work` are one instance; on macOS and Windows the state
directories would otherwise collide on a case-insensitive filesystem while the IDs
differed. The ID, the state folder and the program name use the lowercased name; the
name as typed is kept only for the window title. An invalid name (empty, too long, a dot, a slash, a leading digit) is refused:
the app prints why and exits non-zero. It never falls back to the default instance.

**Parsing.** Decided from raw argv before the application registers, alongside
`app::new_instance_argv` (GTK4Rs/AP-17), and stripped so it never reaches `HANDLES_OPEN` as a
file. Both `--instance NAME` and `--instance=NAME`. `--instance` combined with `-n` is
allowed and means what `-n` always meant: a separate process, here one that uses the named
instance's state.

**Where it reaches.**
- `src/lib.rs` `run`: the ID passed to `Application::builder`. The icon name (`APP_ID` as
  `Icon::App`) does not change.
- `src/session/statedir.rs` `state_directory`: the single state lookup appends
  `instances/NAME` when a name is set. Session, forensics and swap files follow it,
  because they already resolve through it.
- `src/platform/mac/single_instance.rs`: the rendezvous directory is keyed on the name, so
  two named instances on macOS elect separately.
- Anything that lists or prunes the default instance's state folder must leave `instances/`
  alone (8.13). Today the only listing is `forensics::report::announce_unread_report`, which
  reads one level and matches crash-report file names, so a directory never qualifies; the
  swap folder is its own subdirectory. A test pins it: populate `instances/`, run the default
  instance's save, prune and rotation paths, and assert the subtree is byte-identical.
- Windows needs no seat: reuse rides the same D-Bus session bus (TECH, platform table).

**Window identity.** The desktop matches a window to its launcher by the application ID,
not by the window class, and needs no app code for it beyond the ID itself:
- On X11, GTK writes the ID to every window as `_GTK_APPLICATION_ID`. KWin 5.27 and the
  Plasma 5.27 task manager resolve that to `<ID>.desktop` before they ever consult
  `StartupWMClass`. On Wayland the window's `app_id` is the ID. Either way, the launcher
  file must be named `<ID>.desktop`, and KDE must re-index it (`kbuildsycoca5`) after install.
- The X11 window class (`WM_CLASS`) is the program name, `scribobulate`, whatever the ID.
  GTK never derives it from the ID. So for a named instance the app sets the program name
  (`g_set_prgname`) before `Application::run`, in the form

  ```
  <instance-program-name> ::= <program-name> "-" <instance-name>
  ```

  where `<program-name>` is what GLib would otherwise use (the basename of argv[0],
  normally `scribobulate`) and `<instance-name>` is the lowercased name: `scribobulate-work`.
  The launcher's `StartupWMClass` carries the same string, so the class match works too
  when the ID lookup misses. It must be set before the first window exists: a window keeps
  the class it was created with. The default instance's program name is unchanged.

**The launcher.** `packaging/linux/install.sh --instance NAME` writes
`~/.local/share/applications/com.extollit.scribobulate.NAME.desktop`. The file name must
equal the ID for Wayland to match windows to it. It sets `Name=Scribobulate (NAME)`,
`StartupWMClass` to the instance program name (`scribobulate-NAME`), and an `Exec` line carrying `--instance NAME`. Optional
`--state-home DIR` and `--config-home DIR` prefix `Exec` with `env XDG_STATE_HOME=…` /
`XDG_CONFIG_HOME=…`. The launcher registers no MIME types, so double-clicking a `.md`
still opens the default instance. `uninstall.sh --instance NAME` removes it.

**Telling copies apart.** A named instance's window titles carry its name after the app
name, separated by a bar, in the case it was typed: `--instance Work` gives
"notes.md — Scribobulate | Work". The spelling is the one the running instance was started
with; a later launch spelled `work` reaches that instance and does not change its titles.
The default instance's titles are unchanged.

## Proposed TDD Rubrics

To land in TDD §8 after 8.8 (8.8 is taken by "A command-line switch is never opened as a
document").

### 8.9 A named instance reuses only itself
- **Given** a named instance is running
- **When** the application is launched again with the same instance name and a document
- **Then** the document opens in the running named instance, and no new process starts
- **And** a launch with a different name, or with no name, neither reaches nor is reached by it: each starts or reuses its own process

### 8.10 Instance names are checked before anything starts
- **Given** a launch whose instance name is empty, too long, starts with a digit, or contains anything but letters and digits
- **When** it starts
- **Then** it prints why the name is refused, exits with a failure status, and opens no window
- **And** it never falls back to the default instance

### 8.11 An instance's folder name is always lower case
- **Given** a launch whose instance name has upper-case letters (e.g. `--instance Work`)
- **When** it starts and saves its state
- **Then** its folder is `instances/work`, never `instances/Work`, on every platform
- **And** `Work`, `WORK` and `work` select the same instance: one ID, one folder, one process
- **And** the case as typed is kept only for the window title (8.16); the ID, the folder and the program name are lower case

### 8.12 Each named instance keeps its own state
- **Given** two named instances, or a named instance and the default one, with no XDG directory redirected
- **When** each is used and closed
- **Then** each restores only its own windows and documents, and crash recovery offers each only its own unsaved work
- **And** all of them keep that state under the one state home, one folder per instance, readable only by the user
- **And** the settings file (`config.toml`) and the reading-theme definitions (`themes.toml`) are shared by all of them, while what a session remembers (the chosen reading theme, toolbar and split layout, windows and zoom) is each instance's own

### 8.13 The default instance never touches the named instances' state
- **Given** one or more named instances with state under `<state home>/scribobulate/instances/`
- **When** the default instance saves its session, prunes its crash-recovery files, rotates its log, prunes its crash reports, or starts with no saved session
- **Then** nothing under `instances/` is read, changed or deleted
- **And** no named instance touches the default instance's files or another named instance's folder

### 8.14 The XDG directories still choose where things live
- **Given** a named instance launched with `XDG_STATE_HOME` and/or `XDG_CONFIG_HOME` pointing elsewhere
- **When** it runs
- **Then** its state lives under that state home and its settings come from that config home, exactly as for the default instance

### 8.15 A launch without a name is unchanged
- **Given** an existing installation with a saved session
- **When** it is launched without `--instance`
- **Then** it restores that session from where it always was, and reuses or is reused exactly as before (8.1, 8.5)

### 8.16 A named instance says which one it is
- **Given** a named instance
- **When** any of its windows is shown
- **Then** the window title ends with the application name followed by ` | ` and the instance name in the case it was typed with when the instance started (e.g. `--instance Work` gives "notes.md — Scribobulate | Work")
- **And** a later launch that reaches the instance with a different spelling does not change its titles
- **And** a window of the default instance keeps today's title, with no ` | <name>` suffix (e.g. "notes.md — Scribobulate")

### 8.17 A named instance has its own desktop launcher
- **Given** an instance launcher installed for a name (Linux)
- **When** it is started from the desktop, and started again with a document
- **Then** both reach the same named instance, its windows group under that launcher rather than under the default Scribobulate entry, and opening a Markdown file from the file manager still uses the default instance

## Technical details preserved

- `cargo run` pins `XDG_CONFIG_HOME` (`.cargo/config.toml`), and the GTK 4.6 compose-crash
  workaround redirects `XDG_CONFIG_HOME` at startup (GTK4Rs/AP-3, GTK4Rs/AP-173).
  `config::user_config_dir` snapshots the original value before that redirect; the instance
  logic must read config the same way and never through GLib's cached directories.
- The tests redirect `XDG_STATE_HOME` live through `session::with_state_home_for_test`, so
  the instance suffix must be applied in `state_directory`, which reads the variable live,
  not cached at startup.
- Probing a named instance in a test: it is a unique-name owner of its own ID, so
  `org.gtk.Actions` can be addressed at `com.extollit.scribobulate.NAME` without the
  unique-name lookup `-n` launches need (GTK4Rs/AP-253).
- Window matching, source-read (GTK 4.6.9, 4.22.4, 4.24.1; KWin 5.27.5; plasma-workspace
  5.27) and probed on X11 by the `researcher`: `WM_CLASS` res_name is `g_get_prgname()`
  and res_class follows it unless `gdk_x11_display_set_program_class` was called;
  `g_application_run` sets prgname to argv[0]'s basename only if it is unset;
  `_GTK_APPLICATION_ID` comes from the GApplication ID; KWin `manage()` and the task
  manager's `windowUrl` resolve `_GTK_APPLICATION_ID` to a desktop file before the
  `StartupWMClass` comparison (res_name first, then res_class). Today's default launcher
  is `scribobulate.desktop` with `StartupWMClass=com.extollit.scribobulate`, which matches
  nothing; it groups only through the task manager's last fallback, the desktop file's
  own name (`scribobulate`) compared to the window class. Renaming it to
  `com.extollit.scribobulate.desktop` would make the default instance match on the ID as
  well. That rename is optional and outside this plan.
