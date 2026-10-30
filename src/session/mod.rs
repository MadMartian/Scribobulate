//! Cross-session persistence of window/tab layout (TDD 7.2).
//!
//! Stored as a tiny TOML file under `$XDG_STATE_HOME/scribobulate/` (state, not
//! config — this is machine-generated UI state, not user configuration). A
//! STANDALONE window close rewrites the whole file from the current set of live
//! windows (`window::lifecycle::persist_all_windows_session`), so the file always
//! reflects "every window still open right before this one closed" — not just the
//! closing window's own state. A COORDINATED quit (`window::quit_all_windows`)
//! instead snapshots all windows ONCE and FREEZES [`save`] (see [`FROZEN`]) while
//! it closes each window in turn, because `app.windows()` shrinks across that
//! sequence and per-close writes would otherwise leave only the last window
//! (GTK4Rs/AP-113, TDD 15.10).
//!
//! ## Schema (v6)
//!
//! One [`Session`] holds the app-wide values — `preview_theme` (one CSS provider),
//! `play_animations`, the split-pane ARRANGEMENT (`split_swap`, `split_vertical`:
//! one app-wide preference since v5, TDD 7.3) and, since v6, the TOOLBAR LAYOUT
//! (`show_toolbar`, `toolbar_sections`: TDD 9.22) — plus a
//! `Vec<WindowSession>`, one per open window:
//! its geometry, its **one shared zoom level** (zoom is a window-level
//! accessibility setting, not a per-tab one — operator
//! decision), its own [`ChromeSession`] (status bar / sidebar visibility — per
//! window, not app-wide), and a `Vec<TabSession>` (path,
//! view mode, unsafe-images toggle) plus which tab was active. No tab content is persisted
//! (only its `path`, if any) — an unsaved untitled tab restores blank, matching
//! `create_tab_in_window`'s own blank-tab convention (`window::restore`).
//!
//! ## Layout of this module
//!
//! | Submodule | Owns |
//! |---|---|
//! | [`schema`] | the structs the file serialises to and from, and the SCOPE each value is held at |
//! | [`migrate`] | reading files written by superseded schema versions |
//! | [`statedir`] | where the state directory is and how it is created, privately |
//!
//! What stays HERE is the traffic between them: the read/write entry points, the
//! parse that decides which shape a file is, and the save freeze.

mod migrate;
mod schema;
mod statedir;

pub(crate) use schema::{
    sidebar_divider_position, sidebar_split_fraction, ChromeSession, Session, TabSession,
    ToolbarSections, WindowSession,
};
#[cfg(test)]
pub(crate) use statedir::with_state_home_for_test;
pub(crate) use statedir::{create_state_dir, state_directory};

use statedir::session_path;
use std::cell::Cell;

thread_local! {
    /// When set, [`save`] is a no-op. A coordinated app quit
    /// (`window::quit_all_windows`) snapshots the FULL multi-window session ONCE
    /// while every window is alive, then closes each window in turn — but each
    /// close fires its close-request, whose `persist_all_windows_session` would
    /// otherwise re-`save` a SHRINKING window set (the last window to close would
    /// persist only itself, wiping every other window — TDD 15.10). Freezing after
    /// the upfront snapshot keeps that snapshot intact through the close sequence;
    /// a cancelled close (the user aborts quit) un-freezes so normal per-close
    /// persistence resumes. GTK is single-threaded, so a `thread_local` is app-global.
    ///
    /// INVARIANT (QA L-2) — why the thaw is safe with several dirty windows: a
    /// coordinated quit of N windows, ≥2 of them dirty, leaves several Save/Discard/
    /// Cancel dialogs pending at once while `FROZEN == true`. Only an **abort** thaws
    /// — a Cancel/dismiss, or a Save the user backed out of (`window::save`'s confirm
    /// arms: Accept-but-unsaved and the Cancel branch call `set_frozen(false)`); a
    /// successful Save or a Discard proceeds through the close sequence WITHOUT
    /// thawing, so the freeze holds until the quit either completes (all windows
    /// gone) or is aborted. When an abort does thaw, correctness rests on the fact
    /// that `window::lifecycle::persist_all_windows_session` always re-snapshots ALL
    /// still-live windows from `app.windows()` — never just the closing one — so the
    /// resumed per-window `save` re-persists the whole current set, not a shrunk one.
    /// Do NOT change persistence to a single-window write, or thaw on a Save/Discard,
    /// without scoping this freeze to the quit operation itself — either would
    /// reintroduce the TDD-15.10 / GTK4Rs/AP-113 data-loss class this guard exists to
    /// prevent. (Worst case under the current design is bounded: a stale phantom
    /// window restored next launch, never lost edits.)
    static FROZEN: Cell<bool> = const { Cell::new(false) };
}

/// Freeze (`true`) or thaw (`false`) session persistence — see [`FROZEN`].
pub(crate) fn set_frozen(frozen: bool) {
    FROZEN.with(|f| f.set(frozen));
}

/// Parse on-disk TOML text into a [`Session`], transparently migrating older
/// shapes. Split out from [`load`] so the whole decision is unit-testable without
/// touching the filesystem.
///
/// The two arms are the two KINDS of migration, and the split is the only thing
/// this function decides. A v1 file is a wholly different shape and is REPLACED by
/// its migration; every later shape is the current one plus keys that have since
/// moved, so it is parsed normally and then ADDED to. Which superseded versions
/// exist, and what each one relocated, is [`migrate`]'s business alone — a new
/// schema version is added there without touching this.
fn parse(text: &str) -> Session {
    if let Some(session) = migrate::from_v1(text) {
        return session;
    }
    // serde ignores unknown fields, so an older file's now-relocated keys drop here
    // rather than failing the parse (which would discard every window). Recovering
    // them is the next line's job.
    let mut session: Session = toml::from_str(text).unwrap_or_default();
    migrate::apply_superseded(text, &mut session);
    session
}

/// Load the saved session, falling back to defaults when missing or unparseable.
pub(crate) fn load() -> Session {
    let Some(path) = session_path() else {
        return Session::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => parse(&text),
        Err(_) => Session::default(),
    }
}

/// Persist the session (best effort — a write failure is non-fatal, but is
/// now logged rather than silently swallowed, QA round-1 L1: a permissions or
/// full-disk problem used to fail invisibly, leaving no trace for a future bug
/// report to grep). Written via `atomic_io::write_atomic` (QA round-1 M4):
/// `fs::write` truncates then streams, so a crash mid-write left a truncated
/// file that `parse` then reads as a legacy v1 file and migrates to a single
/// default window — every other window's saved layout silently gone.
/// write-temp-then-rename makes that torn-write window impossible.
pub(crate) fn save(session: &Session) {
    // TEST BUILDS ONLY — never compiled into the shipped app.
    //
    // A test must never write the *real* state directory. The integration tests
    // close real windows, which runs `window::lifecycle::persist_all_windows_session`
    // → here, and the path below resolves `XDG_STATE_HOME` live from the environment
    // — so with no override this overwrites the tester's own `session.toml` (their
    // open tabs, window geometry, theme) with whatever the test happened to build.
    // Measured, not theorised.
    //
    // `.cargo/config.toml`'s `[env]` supplies a scratch directory and is the primary
    // defence, but it only covers what Cargo launches. This assertion is the backstop
    // for what it cannot reach: the test binary run directly, an IDE/rust-analyzer
    // runner that bypasses Cargo's config, a CI step that scrubs the environment, or
    // a future edit to that config by someone who does not know what the line guards.
    // Hence the guard lives in the function that does the damage rather than in the
    // callers — there is no test seam on the production path, so a per-test override
    // would have to be remembered by every future window test, and forgetting it
    // fails silently AND destructively (the GTK4Rs/AP-108 shape).
    //
    // It converts that failure into a loud, harmless one. It cannot fire under the
    // normal `cargo test` route, where `[env]` always sets the variable. Scope note:
    // `#[cfg(test)]` covers the in-crate suite (unit + the `gtk-integration-tests`
    // modules), which is where the leak was; a `harness = false` target that does not
    // re-declare the module tree compiles `src/` without `cfg(test)` and so is not
    // covered — such a target must set `XDG_STATE_HOME` itself.
    #[cfg(test)]
    assert!(
        std::env::var_os("XDG_STATE_HOME").is_some(),
        "session::save reached in a test with no XDG_STATE_HOME override — this \
         would overwrite the developer's real session.toml. Run the suite through \
         Cargo so .cargo/config.toml's [env] applies, or set XDG_STATE_HOME to a \
         scratch directory (see session::with_state_home_for_test)."
    );

    // Suppressed during a coordinated quit's window-close sequence so the upfront
    // full-session snapshot is not overwritten by a shrinking set (see [`FROZEN`]).
    if FROZEN.with(|f| f.get()) {
        return;
    }
    let Some(path) = session_path() else { return };
    if let Some(dir) = path.parent() {
        // Through the one seam, so `session.toml` lands inside a private directory —
        // it records the paths of every open document.
        if let Err(e) = create_state_dir(dir) {
            log::warn!("session::save: could not create {}: {e}", dir.display());
            return;
        }
    }
    match toml::to_string(session) {
        Ok(text) => {
            if let Err(e) = crate::atomic_io::write_atomic(&path, &text) {
                log::warn!("session::save: could not write {}: {e}", path.display());
            }
        }
        Err(e) => log::warn!("session::save: could not serialize the session: {e}"),
    }
}

// Serialize tests (in THIS module and any other, e.g. a `gtk-integration-tests`
// module elsewhere) that mutate the process-global `XDG_STATE_HOME`: Rust runs
// tests on parallel (and, for the same worker pool, potentially reused) threads
// and the environment is shared, so concurrent set/remove would flake. Crate-
// visible so every test that needs a scratch session directory shares this ONE
// lock rather than each introducing its own (which would not actually
// serialize against each other).

#[cfg(test)]
mod tests {
    use super::*;
    use schema::sample_session;
    use statedir::with_state_home_for_test as with_state_home;

    /// Every file and folder under `root`, with each file's bytes, in a stable order.
    fn tree(root: &std::path::Path) -> Vec<(std::path::PathBuf, Option<Vec<u8>>)> {
        let mut out = Vec::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let rel = path.strip_prefix(root).unwrap().to_path_buf();
                if path.is_dir() {
                    out.push((rel, None));
                    pending.push(path);
                } else {
                    out.push((rel, Some(std::fs::read(&path).unwrap())));
                }
            }
        }
        out.sort();
        out
    }

    /// TDD 8.13: the default instance's state work never reaches into `instances/`.
    /// Each file planted there has the name of one the default instance itself reads,
    /// writes, rotates or prunes at its own level, so a lookup that strayed one folder
    /// down would find a target.
    #[test]
    fn the_default_instance_leaves_the_named_instances_folder_alone() {
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            // Spelled out, not asked of `state_directory`: the lookup is part of what
            // is under test, and a test that took its paths from it would follow it
            // anywhere (mutation-tested: with the default instance's folder moved into
            // `instances/`, a version reading the paths back stayed green).
            let state = dir.path().join("scribobulate");
            assert_eq!(state_directory(), Some(state.clone()), "TDD 8.15");
            let instances = state.join("instances");
            let named = instances.join("work");
            let swap = crate::swapfile::swap_directory().unwrap();
            std::fs::create_dir_all(named.join(swap.file_name().unwrap())).unwrap();
            for name in [
                "session.toml",
                "scribobulate.log",
                "crash-20260101T000000Z-1.log",
                "crash-last-seen",
            ] {
                std::fs::write(named.join(name), format!("named {name}")).unwrap();
            }
            let before = tree(&instances);

            save(&sample_session());
            save(&Session::default());
            assert_eq!(load(), Session::default());
            crate::forensics::report::announce_unread_report(&state);

            assert_eq!(tree(&instances), before);
            assert!(
                state.join("session.toml").is_file(),
                "the default instance saves at its own level"
            );
        });
    }

    #[test]
    fn save_then_load_round_trips_through_the_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            let s = sample_session();
            save(&s);
            assert!(
                session_path().unwrap().exists(),
                "save() should create <state>/scribobulate/session.toml"
            );
            assert_eq!(load(), s);
        });
    }

    #[test]
    fn frozen_save_is_suppressed_and_thaw_restores_it() {
        // TDD 15.10 regression: `quit_all_windows` snapshots all windows once and
        // freezes, so the per-window close sequence's `save`s must NOT overwrite the
        // snapshot with a shrinking set. A frozen save is a no-op; thawing resumes.
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            let full = sample_session();
            save(&full); // the upfront multi-window snapshot
            assert_eq!(load(), full);

            super::set_frozen(true);
            // A shrinking single-window write, as the last closing window would attempt.
            let shrunk = Session {
                preview_theme: full.preview_theme.clone(),
                play_animations: full.play_animations,
                split_swap: full.split_swap,
                split_vertical: full.split_vertical,
                show_toolbar: full.show_toolbar,
                toolbar_sections: full.toolbar_sections,
                windows: vec![full.windows[0].clone()],
            };
            save(&shrunk);
            assert_eq!(
                load(),
                full,
                "a frozen save must not overwrite the snapshot"
            );

            super::set_frozen(false);
            save(&shrunk);
            assert_eq!(load(), shrunk, "a thawed save persists normally");
        });
    }

    #[test]
    fn load_falls_back_to_default_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            // Nothing written under the fresh state dir → defaults: no windows,
            // so the caller falls back to a single blank welcome window.
            assert!(load().windows.is_empty());
        });
    }

    #[test]
    fn outline_visible_round_trips_through_the_filesystem() {
        // Save/load round-trip, modeled on
        // `save_then_load_round_trips_through_the_filesystem`.
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            let mut s = sample_session();
            s.windows[0].chrome.outline_visible = false;
            save(&s);
            assert!(!load().windows[0].chrome.outline_visible);

            s.windows[0].chrome.outline_visible = true;
            save(&s);
            assert!(load().windows[0].chrome.outline_visible);
        });
    }

    #[test]
    fn two_windows_persist_independent_chrome() {
        // The scope change itself: two windows with different chrome survive a
        // save/load round trip as two different answers, which the app-wide
        // schema could not represent at all.
        let dir = tempfile::tempdir().unwrap();
        with_state_home(dir.path(), || {
            let s = sample_session();
            save(&s);
            let back = load();
            assert!(back.windows[0].chrome.show_statusbar);
            assert!(!back.windows[1].chrome.show_statusbar);
            assert!(!back.windows[0].chrome.outline_visible);
            assert!(back.windows[1].chrome.outline_visible);
        });
    }
}
