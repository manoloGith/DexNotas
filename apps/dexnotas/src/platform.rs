//! What differs between Android and a desktop: the file pickers, sharing, files opened with the
//! app.
//!
//! A picker runs on a thread of its own and its answer comes back through a channel the
//! interface reads every frame.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

/// A picker's answer.
#[derive(Debug)]
pub enum Picked {
    Open(Vec<PathBuf>),
    /// Where to save document `doc`; `close` it afterwards (Save As from "close tab").
    SaveAs { doc: u64, path: PathBuf, close: bool },
}

/// The extensions DexNotas knows, for the pickers' filter.
fn filters() -> Vec<(String, Vec<String>)> {
    Vec::new()
}

/// Ask for files to open.
pub fn pick_open(ctx: &egui::Context, tx: Sender<Picked>) {
    let ctx = ctx.clone();
    #[cfg(target_os = "android")]
    {
        use craft_android::android::pickers::{PickKind, pick};
        let answer = pick(PickKind::OpenFiles, Some(crate::i18n::t("open")), &filters(), None);
        std::thread::spawn(move || {
            let paths = block_on(answer);
            if !paths.is_empty() {
                let _ = tx.send(Picked::Open(paths));
                ctx.request_repaint();
            }
        });
    }
    #[cfg(all(not(target_os = "android"), feature = "desktop"))]
    std::thread::spawn(move || {
        let _ = filters();
        if let Some(paths) = rfd::FileDialog::new().set_title(crate::i18n::t("open")).pick_files() {
            let _ = tx.send(Picked::Open(paths));
            ctx.request_repaint();
        }
    });
    #[cfg(all(not(target_os = "android"), not(feature = "desktop")))]
    {
        let _ = (ctx, tx, filters());
        log::warn!("dexnotas: no file picker in this build");
    }
}

/// Ask where to save document `doc`, suggesting `name`.
pub fn pick_save(ctx: &egui::Context, tx: Sender<Picked>, doc: u64, name: &str, close: bool) {
    let ctx = ctx.clone();
    #[cfg(target_os = "android")]
    {
        use craft_android::android::pickers::{PickKind, pick};
        let answer = pick(PickKind::Save, Some(crate::i18n::t("save_as")), &filters(), Some(name));
        std::thread::spawn(move || {
            if let Some(path) = block_on(answer).into_iter().next() {
                let _ = tx.send(Picked::SaveAs { doc, path, close });
                ctx.request_repaint();
            }
        });
    }
    #[cfg(all(not(target_os = "android"), feature = "desktop"))]
    {
        let name = name.to_owned();
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new().set_title(crate::i18n::t("save_as")).set_file_name(name).save_file() {
                let _ = tx.send(Picked::SaveAs { doc, path, close });
                ctx.request_repaint();
            }
        });
    }
    #[cfg(all(not(target_os = "android"), not(feature = "desktop")))]
    {
        let _ = (ctx, tx, doc, name, close);
        log::warn!("dexnotas: no file picker in this build");
    }
}

/// Files opened with the app since the last call ("Open with", "Share", DexFiles, a drop).
pub fn opened() -> Vec<PathBuf> {
    #[cfg(target_os = "android")]
    {
        craft_android::android::events::opened().into_iter().map(PathBuf::from).collect()
    }
    #[cfg(not(target_os = "android"))]
    Vec::new()
}

/// Hand `text` to another app (Android's share sheet). `false` where there is none.
pub fn share_text(title: &str, text: &str) -> bool {
    #[cfg(target_os = "android")]
    {
        crate::android::share_text(title, text)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (title, text);
        false
    }
}

/// The text on the clipboard (Android: for Paste in a menu; on a desktop the window pastes it).
#[cfg(target_os = "android")]
pub fn clipboard_text() -> Option<String> {
    match craft_android::android::clipboard::get()? {
        craft_android::android::clipboard::Clip::Text(t) => Some(t),
        craft_android::android::clipboard::Clip::Html { text, .. } => Some(text),
        _ => None,
    }
}

#[cfg(not(target_os = "android"))]
pub fn clipboard_text() -> Option<String> {
    None
}

/// A short message from the system (Android), or the log.
pub fn toast(text: &str) {
    #[cfg(target_os = "android")]
    craft_android::android::toast(text);
    #[cfg(not(target_os = "android"))]
    log::info!("{text}");
}

/// Run a future that blocks inside its first poll (craft-android's picker answers).
#[cfg(target_os = "android")]
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, Waker};
    let mut future = std::pin::pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = future.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// The preview can be a web page in the system's WebView (Android). On a desktop there is none
/// (`DEXNOTAS_WEB_LOG=1` pretends there is and logs what it would be told, to test the logic).
pub mod web {
    #[cfg(not(target_os = "android"))]
    fn log_calls() -> bool {
        std::env::var_os("DEXNOTAS_WEB_LOG").is_some()
    }

    pub fn available() -> bool {
        #[cfg(target_os = "android")]
        return true;
        #[cfg(not(target_os = "android"))]
        log_calls()
    }

    pub fn place(visible: bool, rect: [i32; 4]) {
        #[cfg(target_os = "android")]
        crate::android::preview_place(visible, rect);
        #[cfg(not(target_os = "android"))]
        if log_calls() {
            eprintln!("web: place visible={visible} {rect:?}");
        }
    }

    pub fn load(html: &str, base: &str, background: &str, keep_scroll: bool) {
        #[cfg(target_os = "android")]
        crate::android::preview_load(html, base, background, keep_scroll);
        #[cfg(not(target_os = "android"))]
        if log_calls() {
            eprintln!("web: load {} bytes base={base} bg={background} keep={keep_scroll}", html.len());
        }
    }

    pub fn update(html: &str) {
        #[cfg(target_os = "android")]
        crate::android::preview_update(html);
        #[cfg(not(target_os = "android"))]
        if log_calls() {
            eprintln!("web: update {} bytes", html.len());
        }
    }

    pub fn scroll(line: Option<usize>, fraction: f32) {
        #[cfg(target_os = "android")]
        crate::android::preview_scroll(line, fraction);
        #[cfg(not(target_os = "android"))]
        if log_calls() {
            eprintln!("web: scroll line={line:?} fraction={fraction}");
        }
    }

    pub fn clear() {
        #[cfg(target_os = "android")]
        crate::android::preview_clear();
        #[cfg(not(target_os = "android"))]
        if log_calls() {
            eprintln!("web: clear");
        }
    }

    /// Files the page's links asked to open.
    pub fn links() -> Vec<std::path::PathBuf> {
        #[cfg(target_os = "android")]
        return crate::android::preview_links().into_iter().map(std::path::PathBuf::from).collect();
        #[cfg(not(target_os = "android"))]
        Vec::new()
    }
}
