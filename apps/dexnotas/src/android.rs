//! `android_main`: the environment Notepad Studio expects, GPUI's Android platform, and the app.

use std::path::Path;

use android_activity::AndroidApp;
use gpui_mobile::android::jni;

/// Called by android-activity on a thread of its own; returns when the app quits.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("dexnotas"),
    );
    // Panics go to logcat; Notepad Studio adds its own report (panic.log) on top.
    jni::install_panic_hook();

    match app.internal_data_path() {
        Some(data) => set_environment(&data),
        None => log::error!("no internal data path: settings and sessions will not be saved"),
    }

    // GPUI's platform, from the activity. `gpui_platform::application()` in Notepad Studio
    // picks it up; its `run` blocks on the activity's event loop and starts the app once the
    // window has a surface.
    jni::init_platform(&app);
    log::info!("platform ready, starting Notepad Studio");
    notepad_studio::main();
    log::info!("Notepad Studio returned");
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
