package io.github.dexnotas

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.util.Log
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import org.json.JSONObject
import java.io.File

/**
 * The Markdown/HTML preview drawn by the system's WebView (Chrome's engine: CSS, JavaScript,
 * fonts…), over the part of the window the library leaves for it. The library (`src/android.rs`,
 * `ui/webview.rs`) says where it is, what it shows and where to scroll; links to the web open in
 * the browser, links to files come back to the library ([onFile]) to open in a tab.
 *
 * Everything here runs on the UI thread.
 */
class WebPreview(private val activity: Activity, private val surface: () -> View?, private val onFile: (String) -> Unit) {
    private val layer = FrameLayout(activity)
    private var web: WebView? = null
    /** The base address of what is loaded (the document's folder). */
    private var base: String? = null
    /** Scroll to restore after a reload of the same document. */
    private var keepScroll: Int? = null
    /** A page is loading: what arrives meanwhile waits for it. */
    private var loading = false
    private var pendingScroll: JSONObject? = null
    private var pendingHtml: String? = null

    init {
        // A transparent layer over the whole window that lets everything through except the view.
        layer.isClickable = false
        layer.isFocusable = false
        activity.addContentView(layer, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun view(): WebView = web ?: WebView(activity).also { w ->
        w.settings.javaScriptEnabled = true
        w.settings.domStorageEnabled = true
        // Images next to the document; nothing else of the device.
        w.settings.allowFileAccess = true
        w.settings.allowContentAccess = false
        w.settings.builtInZoomControls = true
        w.settings.displayZoomControls = false
        w.isFocusable = true
        w.isFocusableInTouchMode = true
        w.visibility = View.GONE
        w.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean = link(request.url)

            override fun onPageFinished(view: WebView, url: String?) {
                loading = false
                pendingHtml?.let { update(JSONObject().put("html", it)) }
                pendingHtml = null
                val scroll = pendingScroll
                pendingScroll = null
                if (scroll != null) {
                    scroll(scroll)
                } else {
                    keepScroll?.let { y -> view.post { view.scrollTo(0, y) } }
                }
                keepScroll = null
            }
        }
        layer.addView(w, FrameLayout.LayoutParams(1, 1))
        web = w
    }

    /** A link clicked in the page: true when handled here (the page doesn't go anywhere). */
    private fun link(uri: Uri): Boolean {
        return when (uri.scheme?.lowercase()) {
            "http", "https", "mailto", "tel" -> {
                try {
                    activity.startActivity(Intent(Intent.ACTION_VIEW, uri).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                } catch (e: Exception) {
                    Log.w(TAG, "no app for $uri", e)
                }
                true
            }
            "file" -> {
                val path = uri.path
                if (path != null && File(path).isFile) onFile(path)
                true
            }
            else -> true
        }
    }

    /** Where the preview is, in the window's pixels (`visible` false: hidden). */
    fun place(args: JSONObject) {
        val w = if (args.optBoolean("visible")) view() else (web ?: return)
        if (!args.optBoolean("visible")) {
            if (w.hasFocus()) w.clearFocus()
            w.visibility = View.GONE
            return
        }
        // The library's coordinates are the drawing surface's; the layer may start elsewhere.
        val surfaceAt = IntArray(2)
        val layerAt = IntArray(2)
        (surface() ?: activity.window.decorView.findViewById(android.R.id.content))?.getLocationInWindow(surfaceAt)
        layer.getLocationInWindow(layerAt)
        val params = (w.layoutParams as? FrameLayout.LayoutParams) ?: FrameLayout.LayoutParams(1, 1)
        params.width = args.optInt("w").coerceAtLeast(1)
        params.height = args.optInt("h").coerceAtLeast(1)
        params.leftMargin = args.optInt("x") + surfaceAt[0] - layerAt[0]
        params.topMargin = args.optInt("y") + surfaceAt[1] - layerAt[1]
        w.layoutParams = params
        w.visibility = View.VISIBLE
    }

    /** Show a whole page. `keep`: the same document again (keep the scroll position). */
    fun load(args: JSONObject) {
        val w = view()
        val newBase = args.optString("base").ifEmpty { "file:///android_asset/" }
        keepScroll = if (args.optBoolean("keep") && newBase == base) w.scrollY else null
        base = newBase
        loading = true
        pendingHtml = null
        w.setBackgroundColor(parseColor(args.optString("bg")))
        w.loadDataWithBaseURL(newBase, args.optString("html"), "text/html", "utf-8", null)
    }

    /** New content for the page already shown (Markdown): no reload, the scroll stays. */
    fun update(args: JSONObject) {
        if (loading) {
            pendingHtml = args.optString("html")
            return
        }
        web?.evaluateJavascript("typeof dexSet==='function'&&dexSet(${JSONObject.quote(args.optString("html"))})", null)
    }

    /** Follow the editor: to a source line (Markdown) or a fraction of the page (HTML). */
    fun scroll(args: JSONObject) {
        val w = web ?: return
        if (loading) {
            pendingScroll = args
            return
        }
        if (args.has("line")) {
            w.evaluateJavascript("typeof dexLine==='function'&&dexLine(${args.optInt("line")})", null)
        } else {
            w.evaluateJavascript("typeof dexFraction==='function'&&dexFraction(${args.optDouble("fraction")})", null)
        }
    }

    /** Forget the page (the preview was closed). */
    fun clear() {
        web?.let {
            it.visibility = View.GONE
            it.loadUrl("about:blank")
        }
        base = null
        loading = false
        pendingHtml = null
        pendingScroll = null
    }

    /** A press outside the preview gives the keyboard back to the editor. */
    fun touched(e: MotionEvent) {
        val w = web ?: return
        if (e.actionMasked != MotionEvent.ACTION_DOWN || w.visibility != View.VISIBLE || !w.hasFocus()) return
        val at = IntArray(2)
        w.getLocationOnScreen(at)
        val inside = e.rawX >= at[0] && e.rawX < at[0] + w.width && e.rawY >= at[1] && e.rawY < at[1] + w.height
        if (!inside) w.clearFocus()
    }

    /**
     * Keys while the page has the focus: moving, selecting all and copying are the page's; every
     * other key (typing, the app's shortcuts) stays the library's. Returns true when the page took it.
     */
    fun key(e: KeyEvent): Boolean {
        val w = web ?: return false
        if (w.visibility != View.VISIBLE || !w.hasFocus()) return false
        val ctrl = e.isCtrlPressed
        val page = when (e.keyCode) {
            KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN, KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT,
            KeyEvent.KEYCODE_PAGE_UP, KeyEvent.KEYCODE_PAGE_DOWN, KeyEvent.KEYCODE_MOVE_HOME, KeyEvent.KEYCODE_MOVE_END,
            KeyEvent.KEYCODE_SPACE -> !ctrl || e.keyCode == KeyEvent.KEYCODE_MOVE_HOME || e.keyCode == KeyEvent.KEYCODE_MOVE_END
            KeyEvent.KEYCODE_C, KeyEvent.KEYCODE_A -> ctrl && !e.isShiftPressed && !e.isAltPressed
            else -> false
        }
        return page && w.dispatchKeyEvent(e)
    }

    private fun parseColor(s: String): Int = try {
        Color.parseColor(s)
    } catch (e: Exception) {
        Color.WHITE
    }

    companion object {
        private const val TAG = "dexnotas"
    }
}
