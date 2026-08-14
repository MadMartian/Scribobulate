//! macOS-specific seams. Everything here exists because the Quartz backend does
//! not supply something the Linux build gets from the desktop for free — never to
//! give macOS its own *behaviour*.

pub(crate) mod appearance;
pub(crate) mod fullscreen;
pub(crate) mod process;
pub(crate) mod single_instance;
