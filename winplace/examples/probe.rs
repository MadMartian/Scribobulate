//! Show one window in a place read from a TOML file, wait for the window manager to
//! settle, then print the place captured back off it, as TOML, on stdout.
//!
//! The driver for `scripts/winplace-rig.sh`, which runs it under a nested KWin and
//! checks both the captured place and the window's X properties (read independently
//! with `xprop`). `--hold <secs>` keeps the window up that long after printing, so the
//! rig can inspect it; the process then exits on its own. The crate's warnings go to
//! stderr, so the rig can see one.
//!
//! Usage: `probe <place.toml> [--hold <secs>]`

use gtk::glib;
use gtk::prelude::*;
use std::time::Duration;

const SETTLE: Duration = Duration::from_millis(1500);

/// The crate's `warn` and `error` records on stderr.
struct Stderr;

impl log::Log for Stderr {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("{}: {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: probe <place.toml> [--hold <secs>]");
    let hold = match (args.next().as_deref(), args.next()) {
        (Some("--hold"), Some(secs)) => secs.parse().expect("--hold takes seconds"),
        _ => 0u64,
    };
    let text = std::fs::read_to_string(&path).expect("read the place file");
    let place: winplace::Place = toml::from_str(&text).expect("parse the place file");

    log::set_logger(&Stderr).expect("the only logger");
    log::set_max_level(log::LevelFilter::Warn);
    gtk::init().expect("gtk::init");
    let window = gtk::Window::builder().title("winplace-probe").build();
    window.set_child(Some(&gtk::Label::new(Some("winplace probe"))));
    winplace::present(&window, &place);

    let main_loop = glib::MainLoop::new(None, false);
    let ml = main_loop.clone();
    let win = window.clone();
    glib::timeout_add_local_once(SETTLE, move || {
        let captured = winplace::capture(&win);
        println!("{}", toml::to_string(&captured).expect("serialise"));
        let ml = ml.clone();
        glib::timeout_add_local_once(Duration::from_secs(hold), move || ml.quit());
    });
    main_loop.run();
}
