//! The Android side: the entry point, and DexNotas' own bridge to its `MainActivity` (Kotlin)
//! for what craft-android doesn't do: sharing a document's text.
//!
//! Everything else (the JNI bridge to CraftActivity, the environment, mouse, keyboard,
//! lifecycle, pickers, clipboard, "Open with", diagnostics) is craft-android's.

use std::sync::{PoisonError, RwLock};

use craft_android::android::{self as platform, AndroidApp};
use jni::objects::{Global, JObject, JString};
use jni::{EnvUnowned, JValue, JavaVM, jni_sig, jni_str};
use serde_json::json;

/// Entry point, called by android-activity on the activity's own thread.
#[allow(unsafe_code)] // `no_mangle` is how android-activity's glue finds the entry point
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let config = platform::Config { log_tag: "dexnotas", asset_dirs: Vec::new() };
    let Some(started) = platform::start(&app, &config) else {
        return;
    };
    platform::diag(&format!("DexNotas {}", env!("CARGO_PKG_VERSION")));
    let launch = crate::Launch { open: Vec::new(), persistence_path: Some(started.data_dir.join("config/app.ron")) };
    let options_launch = launch.clone();
    crate::run(move |renderer| platform::native_options(&app, crate::native_options(renderer, &options_launch)), launch);
    platform::finish();
}

/// The `MainActivity` (replaced when Android recreates it).
static ACTIVITY: RwLock<Option<Global<JObject<'static>>>> = RwLock::new(None);

/// `MainActivity.notes(op, json)`: its JSON answer ("{}" when there is no activity or it failed).
fn call(op: &str, args: serde_json::Value) -> serde_json::Value {
    let guard = ACTIVITY.read().unwrap_or_else(PoisonError::into_inner);
    let Some(activity) = guard.as_ref() else {
        return json!({});
    };
    let vm = match JavaVM::singleton() {
        Ok(vm) => vm,
        Err(e) => {
            log::warn!("dexnotas: no Java VM for {op}: {e}");
            return json!({});
        }
    };
    let text = args.to_string();
    let result = vm.attach_current_thread(|env| -> jni::errors::Result<String> {
        let op_j = env.new_string(op)?;
        let json_j = env.new_string(&text)?;
        let out = env
            .call_method(
                activity.as_obj(),
                jni_str!("notes"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"),
                &[JValue::Object(&op_j), JValue::Object(&json_j)],
            )?
            .l()?;
        if out.is_null() {
            return Ok("{}".to_owned());
        }
        let s = env.cast_local::<JString>(out)?;
        s.try_to_string(env)
    });
    match result {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| json!({})),
        Err(e) => {
            log::warn!("dexnotas: notes {op} failed: {e}");
            json!({})
        }
    }
}

/// `MainActivity.nativeRegisterNotes(activity)`.
#[allow(unsafe_code)] // `no_mangle`: the JVM finds native methods by their exported name
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexnotas_MainActivity_nativeRegisterNotes<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    activity: JObject<'caller>,
) {
    unowned
        .with_env(|env| -> jni::errors::Result<()> {
            let global = env.new_global_ref(&activity)?;
            *ACTIVITY.write().unwrap_or_else(PoisonError::into_inner) = Some(global);
            Ok(())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

pub fn share_text(title: &str, text: &str) -> bool {
    call("share_text", json!({ "title": title, "text": text })).get("error").is_none()
}

/// The preview's WebView (`WebPreview.kt`): where it is, in the window's pixels.
pub fn preview_place(visible: bool, rect: [i32; 4]) {
    call("preview_place", json!({ "visible": visible, "x": rect[0], "y": rect[1], "w": rect[2], "h": rect[3] }));
}

/// Show a whole page (`base`: the folder its relative links and images are in).
pub fn preview_load(html: &str, base: &str, background: &str, keep_scroll: bool) {
    call("preview_load", json!({ "html": html, "base": base, "bg": background, "keep": keep_scroll }));
}

/// New content for the Markdown page already shown.
pub fn preview_update(html: &str) {
    call("preview_update", json!({ "html": html }));
}

/// Follow the editor: a source line, or a fraction of the page.
pub fn preview_scroll(line: Option<usize>, fraction: f32) {
    match line {
        Some(line) => call("preview_scroll", json!({ "line": line })),
        None => call("preview_scroll", json!({ "fraction": fraction })),
    };
}

pub fn preview_clear() {
    call("preview_clear", json!({}));
}

/// Files the preview's links asked to open, not yet taken.
static PREVIEW_LINKS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn preview_links() -> Vec<String> {
    std::mem::take(&mut *PREVIEW_LINKS.lock().unwrap_or_else(PoisonError::into_inner))
}

/// `MainActivity.nativePreviewLink(path)`: a link in the preview to a file.
#[allow(unsafe_code)] // `no_mangle`: the JVM finds native methods by their exported name
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_dexnotas_MainActivity_nativePreviewLink<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    path: JString<'caller>,
) {
    unowned
        .with_env(|env| -> jni::errors::Result<()> {
            let path = path.try_to_string(env)?;
            PREVIEW_LINKS.lock().unwrap_or_else(PoisonError::into_inner).push(path);
            platform::events::wake();
            Ok(())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}
