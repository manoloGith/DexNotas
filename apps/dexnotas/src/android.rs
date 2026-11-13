//! `android_main`: the environment Notepad Studio expects, GPUI's Android platform, and the app.

use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use android_activity::AndroidApp;
use gpui_mobile::android::jni;

/// Called by android-activity on a thread of its own; returns when the app quits.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let data = app.internal_data_path();
    if let Some(data) = &data {
        start_log(data);
    }
    // One run per process: GPUI must stay on the thread it started on, and this is a new one.
    // (The first run ends the process when it returns, so this should not happen.)
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        eprintln!("android_main called again in the same process: ending it");
        std::process::exit(0);
    }
    log::set_logger(&StderrLogger).ok();
    log::set_max_level(log::LevelFilter::Info);
    // Panics are logged (with where they happened) before the thread unwinds.
    jni::install_panic_hook();
    log::info!("android_main: DexNotas {}", env!("CARGO_PKG_VERSION"));

    match &data {
        Some(data) => set_environment(data),
        None => log::error!("no internal data path: settings and sessions will not be saved"),
    }

    // GPUI's platform, from the activity. `gpui_platform::application()` in Notepad Studio
    // picks it up; its `run` blocks on the activity's event loop and starts the app once the
    // window has a surface.
    jni::init_platform(&app);
    crate::dialogs::install();
    log::info!("platform ready, starting Notepad Studio");
    notepad_studio::main();
    log::info!("Notepad Studio returned");
    // The app has saved its session and the activity is gone. End the process, so the next
    // launch starts a new one instead of calling `android_main` again in this one.
    std::process::exit(0);
}

/// The log of this run, `dexnotas.log` in the app's files (the previous one is kept as
/// `dexnotas.prev.log`): stdout and stderr go there, so it holds what Notepad Studio prints,
/// the `log` records of GPUI's platform and panics. Debug builds show it in the "DexNotas
/// diagnostics" launcher entry.
#[allow(unsafe_code)]
fn start_log(data: &Path) {
    let log = data.join("dexnotas.log");
    let _ = std::fs::rename(&log, data.join("dexnotas.prev.log"));
    let Ok(file) = File::create(&log) else { return };
    // SAFETY: both descriptors are valid for the call; the file stays open through 1 and 2.
    unsafe {
        libc::dup2(file.as_raw_fd(), libc::STDOUT_FILENO);
        libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO);
    }
}

/// `log` records on stderr, which [`start_log`] points at the log file.
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("{:<5} {}: {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

/// Notepad Studio finds its settings, sessions, logs and temporary files through `HOME` and the
/// XDG variables (the `dirs` crate): point them at the app's private storage.
#[allow(unsafe_code)]
fn set_environment(data: &Path) {
    let dirs = [
        ("HOME", data.to_path_buf()),
        ("XDG_CONFIG_HOME", data.join("config")),
        ("XDG_DATA_HOME", data.join("data")),
        ("XDG_STATE_HOME", data.join("state")),
        ("XDG_CACHE_HOME", data.join("cache")),
        ("TMPDIR", data.join("tmp")),
    ];
    for (name, dir) in dirs {
        if let Err(error) = std::fs::create_dir_all(&dir) {
            log::error!("cannot create {}: {error}", dir.display());
        }
        // SAFETY: `android_main` runs this before it starts any thread of its own, and nothing
        // else in the process reads these variables yet.
        unsafe { std::env::set_var(name, &dir) };
    }
}
