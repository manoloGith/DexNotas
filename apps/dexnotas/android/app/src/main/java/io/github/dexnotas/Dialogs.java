package io.github.dexnotas;

import android.app.Activity;
import android.app.AlertDialog;

/**
 * Message boxes for the native side (apps/dexnotas/src/dialogs.rs): Notepad Studio asks with
 * them whether to save before closing, to reload a file that changed, and reports errors.
 */
public final class Dialogs {
    private Dialogs() {}

    /** The answer to the box `id`: the index of the button, or -1 when it was dismissed. */
    private static native void nativeAnswer(long id, int button);

    /** Called from the native thread. `negative` and `neutral` are empty for no such button. */
    public static void message(Activity activity, long id, String title, String text,
            String positive, String negative, String neutral) {
        activity.runOnUiThread(() -> {
            if (activity.isFinishing() || activity.isDestroyed()) {
                nativeAnswer(id, -1);
                return;
            }
            boolean[] answered = {false};
            AlertDialog.Builder box = new AlertDialog.Builder(activity).setTitle(title).setMessage(text);
            box.setPositiveButton(positive, (dialog, which) -> { answered[0] = true; nativeAnswer(id, 0); });
            if (!negative.isEmpty()) {
                box.setNegativeButton(negative, (dialog, which) -> { answered[0] = true; nativeAnswer(id, 1); });
            }
            if (!neutral.isEmpty()) {
                box.setNeutralButton(neutral, (dialog, which) -> { answered[0] = true; nativeAnswer(id, 2); });
            }
            box.setOnDismissListener(dialog -> { if (!answered[0]) nativeAnswer(id, -1); });
            box.show();
        });
    }
}
