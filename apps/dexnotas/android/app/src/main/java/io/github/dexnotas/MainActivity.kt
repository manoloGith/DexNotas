package io.github.dexnotas

import android.content.Intent
import android.os.Bundle
import android.util.Log
import io.github.craftandroid.CraftActivity
import org.json.JSONObject

/**
 * DexNotas' activity. Mouse, keyboard, lifecycle, the Storage Access Framework (open, save, save
 * as, with the changes written back to the document), clipboard and diagnostics are
 * craft-android's [CraftActivity]; the library it loads (`libdexnotas.so`) draws the whole window.
 *
 * What an editor needs besides is here, called by the library through [notes]
 * (`src/android.rs`): sharing a document's text with other apps.
 */
class MainActivity : CraftActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (!isFinishing) nativeRegisterNotes(this)
    }

    /** Text files dropped on the window or shared to the app open in tabs. */
    override fun droppableMimeTypes(): List<String> =
        listOf("text/*", "application/json", "application/xml", "application/javascript", "application/x-sh", "application/octet-stream")

    /** Called by the library (any thread) with an operation and JSON; answers JSON. Never throws. */
    @Suppress("unused") // called over JNI
    fun notes(op: String, json: String): String = try {
        val args = if (json.isEmpty()) JSONObject() else JSONObject(json)
        when (op) {
            "share_text" -> {
                val intent = Intent(Intent.ACTION_SEND)
                    .setType("text/plain")
                    .putExtra(Intent.EXTRA_SUBJECT, args.optString("title"))
                    .putExtra(Intent.EXTRA_TEXT, args.optString("text"))
                runOnUiThread {
                    try {
                        startActivity(Intent.createChooser(intent, args.optString("title")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                    } catch (e: Exception) {
                        Log.w(TAG, "share failed", e)
                    }
                }
                "{}"
            }
            else -> { Log.w(TAG, "notes: unknown operation $op"); "{}" }
        }
    } catch (e: Exception) {
        Log.w(TAG, "notes $op failed", e)
        JSONObject().put("error", e.toString()).toString()
    }

    private external fun nativeRegisterNotes(activity: MainActivity)

    companion object {
        private const val TAG = "dexnotas"
    }
}
