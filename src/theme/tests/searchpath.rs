//! The theme search path — SCHEMA § Search path's three rows, their order, and what
//! "first match wins" actually means.
//!
//! **Every rule in that section was previously untested.** `find_themes_file` read the
//! three bases inline and took no parameters, so the only way to exercise it was to
//! mutate `std::env` — process-global, and a hazard rather than a test. SCHEMA records
//! that the omission is not hypothetical: the row-1 spelling once made user theme
//! overrides unreachable on Windows outright.
//!
//! The seam is a **data** one (`SearchBases`), so these are ordinary display-free unit
//! tests over real temp directories, safe to run in parallel with everything else.

use super::super::{read_first_existing, SearchBases};
use std::path::{Path, PathBuf};

/// A `SearchBases` over three distinct temp directories, so a candidate's ROW is
/// identifiable from the path it produced.
fn bases(config: &Path, data_home: &Path, system: &[&Path]) -> SearchBases {
    SearchBases {
        config: Some(config.to_path_buf()),
        data_home: data_home.to_path_buf(),
        system_dirs: system.iter().map(|p| p.to_path_buf()).collect(),
    }
}

/// Put a themes file under `base/scribobulate/`, the layout every row shares.
fn plant(base: &Path, body: &str) -> PathBuf {
    let dir = base.join("scribobulate");
    std::fs::create_dir_all(&dir).expect("create the candidate directory");
    let file = dir.join("themes.toml");
    std::fs::write(&file, body).expect("write the candidate file");
    file
}

/// SCHEMA's three rows, in SCHEMA's order, each with the `scribobulate/themes.toml`
/// leaf — and `$XDG_DATA_DIRS` **iterated**, one candidate per entry.
#[test]
fn the_candidate_order_is_the_three_rows_the_schema_states() {
    let c = PathBuf::from("/c");
    let d = PathBuf::from("/d");
    let s1 = PathBuf::from("/s1");
    let s2 = PathBuf::from("/s2");
    let got = bases(&c, &d, &[&s1, &s2]).candidates();
    assert_eq!(
        got,
        vec![
            c.join("scribobulate").join("themes.toml"),
            d.join("scribobulate").join("themes.toml"),
            s1.join("scribobulate").join("themes.toml"),
            s2.join("scribobulate").join("themes.toml"),
        ]
    );
}

/// `$XDG_DATA_DIRS` is a LIST and every entry is a candidate — never hard-coded to
/// `/usr/share`. On a KDE box the first entry is `/usr/share/plasma`, so a hard-coded
/// path works on GNOME and silently fails there.
#[test]
fn every_system_data_dir_becomes_a_candidate_in_its_own_order() {
    let dirs = ["/usr/share/plasma", "/usr/local/share", "/usr/share"];
    let refs: Vec<PathBuf> = dirs.iter().map(PathBuf::from).collect();
    let bases = SearchBases {
        config: None,
        data_home: PathBuf::from("/home/u/.local/share"),
        system_dirs: refs.clone(),
    };
    let got = bases.candidates();
    assert_eq!(got.len(), 1 + dirs.len(), "one candidate per row, in order");
    for (i, dir) in refs.iter().enumerate() {
        assert_eq!(got[i + 1], dir.join("scribobulate").join("themes.toml"));
    }
}

/// A host with no resolvable config directory drops row 1 rather than inventing one.
#[test]
fn an_absent_config_directory_drops_row_one_and_nothing_else() {
    let bases = SearchBases {
        config: None,
        data_home: PathBuf::from("/d"),
        system_dirs: vec![PathBuf::from("/s")],
    };
    let got = bases.candidates();
    assert_eq!(got.len(), 2);
    assert!(got[0].starts_with("/d"));
}

/// **First match wins — a later file does NOT merge over an earlier one.**
///
/// This is the rule most likely to be got backwards, because the *theme* merge one
/// level up IS a merge. A system install must not be able to add keys to a user's own
/// themes file; it is shadowed whole.
#[test]
fn the_first_existing_candidate_wins_whole_and_later_ones_are_not_merged() {
    let cfg = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let sys = tempfile::tempdir().unwrap();
    plant(cfg.path(), "# row 1\n[themes.a]\nname = \"One\"\n");
    plant(data.path(), "# row 2\n[themes.b]\nname = \"Two\"\n");
    plant(sys.path(), "# row 3\n[themes.c]\nname = \"Three\"\n");

    let (text, _dir) =
        read_first_existing(&bases(cfg.path(), data.path(), &[sys.path()]).candidates())
            .expect("row 1 exists");
    assert!(text.contains("row 1"));
    assert!(
        !text.contains("row 2") && !text.contains("row 3"),
        "a later candidate must be shadowed whole, not merged over"
    );
}

/// Each row is reachable in turn: remove the row above it and the next one answers.
#[test]
fn each_row_answers_when_the_rows_above_it_are_absent() {
    let cfg = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let sys = tempfile::tempdir().unwrap();
    let row1 = plant(cfg.path(), "# row 1\n");
    let row2 = plant(data.path(), "# row 2\n");
    plant(sys.path(), "# row 3\n");
    let all = bases(cfg.path(), data.path(), &[sys.path()]).candidates();

    assert!(read_first_existing(&all).unwrap().0.contains("row 1"));
    std::fs::remove_file(&row1).unwrap();
    assert!(read_first_existing(&all).unwrap().0.contains("row 2"));
    std::fs::remove_file(&row2).unwrap();
    assert!(read_first_existing(&all).unwrap().0.contains("row 3"));
}

/// **The directory returned is the found file's own parent**, because that is the
/// sprite origin — a theme file's `*_sprite` references resolve against the directory
/// it was read from, never against the process's working directory.
#[test]
fn the_directory_reported_is_the_found_files_own_parent() {
    let cfg = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    plant(data.path(), "# row 2\n");
    let (_text, dir) =
        read_first_existing(&bases(cfg.path(), data.path(), &[]).candidates()).expect("row 2");
    assert_eq!(dir, data.path().join("scribobulate"));
    assert_ne!(
        dir,
        cfg.path().join("scribobulate"),
        "the origin must be the directory the file WAS IN, not the first row tried"
    );
}

/// No candidate at all is `None`, not a panic and not an empty-string file — the
/// compiled-in themes stand alone on a host with nothing installed.
#[test]
fn no_candidate_anywhere_answers_none() {
    let empty = tempfile::tempdir().unwrap();
    let got = read_first_existing(
        &bases(
            &empty.path().join("nope-a"),
            &empty.path().join("nope-b"),
            &[&empty.path().join("nope-c")],
        )
        .candidates(),
    );
    assert!(got.is_none());
}
