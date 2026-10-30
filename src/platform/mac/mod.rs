//! macOS-specific seams. Everything here exists because the Quartz backend does
//! not supply something the Linux build gets from the desktop for free — never to
//! give macOS its own *behaviour*.

pub(crate) mod appearance;
pub(crate) mod automount;
pub(crate) mod bundle;
/// Which process is frontmost. Test-only: no product path asks.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) mod frontmost;
pub(crate) mod fullscreen;
pub(crate) mod menubar;
pub(crate) mod pasteboard;
pub(crate) mod pointercrossing;
pub(crate) mod process;
pub(crate) mod single_instance;
