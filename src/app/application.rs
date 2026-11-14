//! `ScribobulateApplication`: `gtk::Application` with ONE override, the
//! `local_command_line` vfunc, so the command line GLib actually parses is shaped by
//! [`super::shape_argv`] on every platform (TDD 8.8, 8.5, 8.9).
//!
//! # Why the argv handed to `run_with_args` is not enough
//!
//! On Windows `g_application_run` IGNORES the argv it is given. It re-reads the
//! process command line with `g_win32_get_command_line()` and, when that has more
//! entries than `argc`, drops the surplus from the FRONT (`gio/gapplication.c`; the
//! trim exists for interpreters such as `python script.py`). Only the COUNT of what
//! `run()` passes survives. So shaping in `run()` alone shortened `argc` and shifted
//! the real command line left: `scribobulate -- -x.md` reached GLib as
//! `["--", "-x.md"]`, the `--` became the program name and `-x.md` was refused as an
//! unknown option; `scribobulate file.md -n` reached it as `["file.md", "-n"]` and was
//! refused the same way. Measured on Windows, GLib 2.90.1.
//!
//! The vfunc is the first point that sees the arguments GLib will really parse, on
//! every platform, so the shaping is repeated there. On Linux and macOS it receives
//! what `run()` passed, already shaped, and `shape_argv` is idempotent, so the
//! override leaves the vector untouched and chains on (asserted by
//! `shape_argv_tests::shaping_is_idempotent`). On Windows `run()` passes the unshaped
//! argv, whose count matches the real command line, so GLib trims nothing.
//!
//! # Why the vfunc is set by hand
//!
//! gtk-rs offers `ApplicationImpl::local_command_line`, but its `ArgumentList` can only
//! REMOVE entries, and the `--` rule must rewrite one (`-x.md` becomes `.\-x.md`). So
//! `class_init` installs a C function over the trampoline gtk-rs put there, replaces
//! the string vector in place, and chains to the parent class's own function. Nothing
//! else about the application changes; every other site holds it as a plain
//! `gtk::Application`.

use std::ffi::{c_char, c_int, CStr};

use gtk::gio;
use gtk::glib;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Default)]
    pub(crate) struct ScribobulateApplication;

    #[glib::object_subclass]
    impl ObjectSubclass for ScribobulateApplication {
        const NAME: &'static str = "ScribobulateApplication";
        type Type = super::ScribobulateApplication;
        type ParentType = gtk::Application;

        fn class_init(klass: &mut Self::Class) {
            // SAFETY: `Self::Class` is `#[repr(C)]` and begins with the parent class
            // struct, `GtkApplicationClass`, which begins with `GApplicationClass`; so
            // the pointer is a valid `GApplicationClass` for the duration of class
            // initialisation. glib runs the parent's `class_init` (gtk-rs's trampolines)
            // BEFORE this one, so this assignment is the one that stays.
            unsafe {
                let class = (klass as *mut Self::Class).cast::<gio::ffi::GApplicationClass>();
                (*class).local_command_line = Some(local_command_line);
            }
        }
    }

    impl ObjectImpl for ScribobulateApplication {}
    impl ApplicationImpl for ScribobulateApplication {}
    impl GtkApplicationImpl for ScribobulateApplication {}

    /// `GApplicationClass::local_command_line`: shape `*arguments`, then chain.
    unsafe extern "C" fn local_command_line(
        application: *mut gio::ffi::GApplication,
        arguments: *mut *mut *mut c_char,
        exit_status: *mut c_int,
    ) -> glib::ffi::gboolean {
        // SAFETY: GLib passes the address of the NULL-terminated, g_malloc'd string
        // vector it owns and frees with `g_strfreev` after this returns.
        unsafe { reshape(arguments) };
        let parent = ScribobulateApplication::type_data()
            .as_ref()
            .parent_class()
            .cast::<gio::ffi::GApplicationClass>();
        // SAFETY: `parent` is the parent class, `GtkApplicationClass`, kept alive by
        // the type system for the life of the process.
        match unsafe { (*parent).local_command_line } {
            Some(chain) => unsafe { chain(application, arguments, exit_status) },
            None => glib::ffi::GFALSE,
        }
    }

    /// Replace `*arguments` with `shape_argv` of it. Leaves it alone when it is not
    /// all UTF-8, or when shaping changes nothing (every launch on Linux and macOS).
    ///
    /// # Safety
    ///
    /// `arguments` must be null or point at a NULL-terminated, g_malloc'd string
    /// vector that the caller frees with `g_strfreev`.
    unsafe fn reshape(arguments: *mut *mut *mut c_char) {
        // SAFETY: per this function's contract.
        let strv = unsafe {
            if arguments.is_null() || (*arguments).is_null() {
                return;
            }
            *arguments
        };
        let mut args = Vec::new();
        for index in 0.. {
            // SAFETY: the vector is NULL-terminated, and reading stops at the NULL.
            let item = unsafe { *strv.add(index) };
            if item.is_null() {
                break;
            }
            // SAFETY: each entry is a NUL-terminated C string.
            match unsafe { CStr::from_ptr(item) }.to_str() {
                Ok(text) => args.push(text.to_owned()),
                Err(_) => return,
            }
        }
        let shaped = crate::app::shape_argv(args.clone());
        if shaped == args {
            return;
        }
        // SAFETY: the old vector is GLib's and is freed exactly once, here; the new one
        // is g_malloc'd by `StrV`, and GLib frees it with `g_strfreev` in its place.
        unsafe {
            glib::ffi::g_strfreev(strv);
            *arguments = glib::StrV::from(shaped).into_raw();
        }
    }
}

glib::wrapper! {
    pub(crate) struct ScribobulateApplication(ObjectSubclass<imp::ScribobulateApplication>)
        @extends gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl ScribobulateApplication {
    /// The application object `run()` builds, with the ID and flags it chose.
    pub(crate) fn new(application_id: &str, flags: gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .build()
    }
}
