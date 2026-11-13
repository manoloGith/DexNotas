//! Notepad Studio's message boxes on Android.
//!
//! Notepad Studio asks through `rfd` (in the fork, a shim that asks the backend installed here).
//! A box is an `AlertDialog` shown by `io.github.dexnotas.Dialogs` on the UI thread; its answer
//! comes back through [`Java_io_github_dexnotas_Dialogs_nativeAnswer`].
//!
//! File pickers are not here yet: opening and saving answer "cancelled".

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use futures_channel::oneshot;
use gpui_mobile::android::jni as platform;
use jni::objects::JValue;
use rfd::{Answer, Kind, Message, MessageButtons, MessageDialogResult, Request};

/// Install the backend (once, before Notepad Studio starts).
pub fn install() {
    if !rfd::set_backend(Box::new(AndroidDialogs)) {
        log::warn!("a dialog backend was already installed");
    }
}

struct AndroidDialogs;

impl rfd::Backend for AndroidDialogs {
    fn pick(&self, kind: Kind, request: Request) -> Answer<Vec<PathBuf>> {
        log::warn!("file picker not implemented yet: {kind:?} {:?} is cancelled", request.title);
        Box::pin(async { Vec::new() })
    }

    fn message(&self, message: Message) -> Answer<MessageDialogResult> {
        let buttons = Buttons::of(&message.buttons);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        pending().push((id, sender));
        if let Err(error) = show(id, &message, &buttons) {
            log::error!("cannot show the message box \"{}\": {error}", message.title);
            take(id);
            return Box::pin(async move { buttons.dismissed });
        }
        Box::pin(async move {
            match receiver.await {
                Ok(index) => buttons.results.get(index as usize).cloned().unwrap_or(buttons.dismissed),
                Err(_) => buttons.dismissed,
            }
        })
    }
}

/// The labels of a box's buttons (positive, negative, neutral), what each one answers, and what
/// closing the box without choosing answers.
struct Buttons {
    labels: Vec<String>,
    results: Vec<MessageDialogResult>,
    dismissed: MessageDialogResult,
}

impl Buttons {
    fn of(buttons: &MessageButtons) -> Self {
        use MessageDialogResult as R;
        let custom = |labels: &[&String]| Buttons {
            labels: labels.iter().map(|l| (*l).clone()).collect(),
            results: labels.iter().map(|l| R::Custom((*l).clone())).collect(),
            dismissed: R::Cancel,
        };
        let fixed = |labels: &[&str], results: Vec<R>, dismissed: R| Buttons {
            labels: labels.iter().map(|l| (*l).to_owned()).collect(),
            results,
            dismissed,
        };
        match buttons {
            MessageButtons::Ok => fixed(&["OK"], vec![R::Ok], R::Ok),
            MessageButtons::OkCancel => fixed(&["OK", "Cancel"], vec![R::Ok, R::Cancel], R::Cancel),
            MessageButtons::YesNo => fixed(&["Yes", "No"], vec![R::Yes, R::No], R::No),
            MessageButtons::YesNoCancel => fixed(&["Yes", "No", "Cancel"], vec![R::Yes, R::No, R::Cancel], R::Cancel),
            MessageButtons::OkCustom(ok) => custom(&[ok]),
            MessageButtons::OkCancelCustom(ok, cancel) => custom(&[ok, cancel]),
            MessageButtons::YesNoCancelCustom(yes, no, cancel) => custom(&[yes, no, cancel]),
        }
    }
}

static NEXT_ID: AtomicI64 = AtomicI64::new(1);
/// The boxes that are up: their id and where the answer goes.
static PENDING: Mutex<Vec<(i64, oneshot::Sender<i32>)>> = Mutex::new(Vec::new());

fn pending() -> std::sync::MutexGuard<'static, Vec<(i64, oneshot::Sender<i32>)>> {
    PENDING.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Take the box `id` out of the pending ones.
fn take(id: i64) -> Option<oneshot::Sender<i32>> {
    let mut boxes = pending();
    let index = boxes.iter().position(|(pending_id, _)| *pending_id == id)?;
    Some(boxes.swap_remove(index).1)
}

fn show(id: i64, message: &Message, buttons: &Buttons) -> Result<(), String> {
    let label = |index: usize| buttons.labels.get(index).cloned().unwrap_or_default();
    platform::with_env(|env| {
        let activity = platform::activity(env)?;
        let class = platform::find_app_class(env, "io.github.dexnotas.Dialogs")?;
        let text = |env: &mut jni::Env, s: &str| env.new_string(s).map_err(|e| e.to_string());
        let title = text(env, &message.title)?;
        let description = text(env, &message.description)?;
        let positive = text(env, &label(0))?;
        let negative = text(env, &label(1))?;
        let neutral = text(env, &label(2))?;
        env.call_static_method(
            &class,
            jni::jni_str!("message"),
            jni::jni_sig!(
                "(Landroid/app/Activity;JLjava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V"
            ),
            &[
                JValue::Object(&activity),
                JValue::Long(id),
                JValue::Object(&title),
                JValue::Object(&description),
                JValue::Object(&positive),
                JValue::Object(&negative),
                JValue::Object(&neutral),
            ],
        )
        .map_err(|e| {
            env.exception_clear();
            e.to_string()
        })?;
        Ok(())
    })
}

/// `Dialogs.nativeAnswer(id, button)`, on the UI thread.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn Java_io_github_dexnotas_Dialogs_nativeAnswer(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    id: i64,
    button: i32,
) {
    if let Some(sender) = take(id) {
        let _ = sender.send(button);
    }
}
