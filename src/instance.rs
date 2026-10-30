//! Named instances: `--instance NAME` (TDD 8.9–8.17).
//!
//! A launch without the switch is the default instance and is exactly the app as it
//! was before this module existed: the same application ID, the same state folder, the
//! same program name and the same titles (TDD 8.15). A named instance changes four
//! things, each derived here from one parsed [`InstanceName`] so they cannot disagree:
//!
//! | What | Default | `--instance Work` |
//! |---|---|---|
//! | Application ID (uniqueness) | `com.extollit.scribobulate` | `com.extollit.scribobulate.work` |
//! | State folder | `<state home>/scribobulate/` | `<state home>/scribobulate/instances/work/` |
//! | Program name (X11 `WM_CLASS`) | `scribobulate` | `scribobulate-work` |
//! | Window title suffix | none | ` \| Work` |
//!
//! Uniqueness is keyed by the application ID, not by any directory, which is why a
//! second reusable copy needs a second ID and why redirecting `XDG_STATE_HOME` alone
//! never separated copies.
//!
//! The name is decided from raw argv before the application registers
//! (GTK4Rs/AP-17) and before logging starts, because the persistent log and the crash
//! reports live in the state folder and must land in the instance's own. It is held
//! once for the process in [`select`]/[`current`]; everything else reads it there.
//!
//! **Case.** The ID, the folder and the program name use the lower-cased name, so
//! `Work` and `work` are one instance whatever the filesystem's case rules (TDD 8.11).
//! The name as typed survives only in the window title (TDD 8.16).

use std::fmt;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The command-line switch. Also accepted as `--instance=NAME`.
pub(crate) const SWITCH: &str = "--instance";

/// Longest accepted name, in characters (all ASCII, so also bytes).
pub(crate) const MAX_NAME_LEN: usize = 32;

/// The subfolder of the default instance's state folder that holds one folder per
/// named instance. Nothing of the default instance's own lives in it (TDD 8.13).
pub(crate) const INSTANCES_DIR: &str = "instances";

/// Separates the program name from the instance name in a named instance's
/// program name: `<program-name> "-" <instance-name>`.
const PROGRAM_NAME_SEPARATOR: char = '-';

/// Separates the application name from the instance name in a window title.
const TITLE_SEPARATOR: &str = " | ";

/// A validated instance name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstanceName {
    /// Lower-cased: the ID element, the folder name and the program-name suffix.
    key: String,
    /// As typed: shown in window titles only.
    shown: String,
}

/// Why a name was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NameError {
    Empty,
    TooLong(usize),
    /// A D-Bus name element may not start with a digit.
    LeadingDigit,
    Disallowed(char),
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "it is empty"),
            Self::TooLong(len) => {
                write!(
                    f,
                    "it is {len} characters long; the limit is {MAX_NAME_LEN}"
                )
            }
            Self::LeadingDigit => write!(f, "it starts with a digit; it must start with a letter"),
            Self::Disallowed(c) => {
                write!(
                    f,
                    "it contains {c:?}; only ASCII letters and digits are allowed"
                )
            }
        }
    }
}

impl InstanceName {
    /// Validate `raw`: 1 to [`MAX_NAME_LEN`] ASCII letters and digits, starting with a
    /// letter. Never repaired: an invalid name is refused, not narrowed into a valid one.
    pub(crate) fn parse(raw: &str) -> Result<Self, NameError> {
        let Some(first) = raw.chars().next() else {
            return Err(NameError::Empty);
        };
        if let Some(bad) = raw.chars().find(|c| !c.is_ascii_alphanumeric()) {
            return Err(NameError::Disallowed(bad));
        }
        if first.is_ascii_digit() {
            return Err(NameError::LeadingDigit);
        }
        if raw.len() > MAX_NAME_LEN {
            return Err(NameError::TooLong(raw.len()));
        }
        Ok(Self {
            key: raw.to_ascii_lowercase(),
            shown: raw.to_owned(),
        })
    }

    /// The lower-cased name: identity everywhere except the title.
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    /// The name as typed, for display.
    pub(crate) fn shown(&self) -> &str {
        &self.shown
    }
}

/// Why the command line's instance switch was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArgError {
    MissingName,
    Repeated,
    Invalid { raw: String, why: NameError },
}

impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingName => write!(f, "{SWITCH} needs a name, e.g. {SWITCH} work"),
            Self::Repeated => write!(f, "{SWITCH} was given more than once"),
            Self::Invalid { raw, why } => {
                write!(f, "the instance name {raw:?} is refused: {why}")
            }
        }
    }
}

/// Take the instance switch out of argv.
///
/// Returns the name (`None` for the default instance) and argv without the switch and
/// its value, so what reaches `HANDLES_OPEN` is file paths only. A bare `--` ends
/// option parsing, as for every other switch (`app::carries_option_argument`): after
/// it, `--instance` is a file name. `argv[0]` is the program and is never examined.
pub(crate) fn instance_argv(
    args: Vec<String>,
) -> Result<(Option<InstanceName>, Vec<String>), ArgError> {
    let mut name = None;
    let mut rest = Vec::with_capacity(args.len());
    let mut iter = args.into_iter();
    rest.extend(iter.next());
    let prefixed = format!("{SWITCH}=");
    while let Some(arg) = iter.next() {
        let raw = if arg == "--" {
            rest.push(arg);
            rest.extend(iter.by_ref());
            break;
        } else if arg == SWITCH {
            iter.next().ok_or(ArgError::MissingName)?
        } else if let Some(value) = arg.strip_prefix(&prefixed) {
            value.to_owned()
        } else {
            rest.push(arg);
            continue;
        };
        if name.is_some() {
            return Err(ArgError::Repeated);
        }
        let parsed = InstanceName::parse(&raw).map_err(|why| ArgError::Invalid { raw, why })?;
        name = Some(parsed);
    }
    Ok((name, rest))
}

static CURRENT: OnceLock<Option<InstanceName>> = OnceLock::new();

/// Record this process's instance. Called once, from `run`, before logging starts.
pub(crate) fn select(name: Option<InstanceName>) {
    if CURRENT.set(name).is_err() {
        panic!("instance::select called twice");
    }
}

/// This process's instance, or `None` for the default one (and in tests, which never
/// select one).
pub(crate) fn current() -> Option<&'static InstanceName> {
    CURRENT.get().and_then(Option::as_ref)
}

/// The application ID: `base` for the default instance, `base.<key>` for a named one.
pub(crate) fn app_id_for(base: &str, name: Option<&InstanceName>) -> String {
    match name {
        Some(name) => format!("{base}.{}", name.key()),
        None => base.to_owned(),
    }
}

/// The program name to set before the application runs, or `None` to leave GLib's
/// own (the default instance). `program` is `argv[0]`; GLib would use its file name.
pub(crate) fn program_name_for(program: &str, name: Option<&InstanceName>) -> Option<String> {
    let name = name?;
    let base = std::path::Path::new(program)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_owned());
    Some(format!("{base}{PROGRAM_NAME_SEPARATOR}{}", name.key()))
}

/// The instance's state folder, given the default instance's.
pub(crate) fn state_dir_for(app_state: PathBuf, name: Option<&InstanceName>) -> PathBuf {
    match name {
        Some(name) => app_state.join(INSTANCES_DIR).join(name.key()),
        None => app_state,
    }
}

/// What a window title ends with after the application name: nothing for the default
/// instance, ` | <name as typed>` for a named one.
pub(crate) fn title_suffix(name: Option<&InstanceName>) -> String {
    name.map(|name| format!("{TITLE_SEPARATOR}{}", name.shown()))
        .unwrap_or_default()
}

/// A short, stable tag for a named instance, for names that must fit a tight limit
/// (the macOS rendezvous socket path is capped at 104 bytes, and a 32-character name
/// would overrun it). FNV-1a over the key: fixed by this function, not by the
/// standard library's hasher, whose output may change between Rust releases, because
/// two different builds must derive the same tag to find each other.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // only the macOS rendezvous uses it; tested everywhere
pub(crate) fn short_tag(name: &InstanceName) -> String {
    const OFFSET: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let hash = name
        .key()
        .bytes()
        .fold(OFFSET, |h, b| (h ^ u32::from(b)).wrapping_mul(PRIME));
    format!("{hash:08x}")
}

#[cfg(test)]
mod tests;
