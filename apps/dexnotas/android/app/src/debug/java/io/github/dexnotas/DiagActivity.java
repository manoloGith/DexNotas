package io.github.dexnotas;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Intent;
import android.graphics.Typeface;
import android.os.Build;
import android.os.Bundle;
import android.util.DisplayMetrics;
import android.view.Gravity;
import android.view.ViewGroup.LayoutParams;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

import java.io.File;
import java.io.RandomAccessFile;
import java.nio.charset.StandardCharsets;

/**
 * Shows what the native side wrote: this run's log and the previous run's. The files are in the app's private storage (see apps/dexnotas/src/android.rs).
 */
public final class DiagActivity extends Activity {
    /** Enough to see a start-up; the end of a file is what matters. */
    private static final int MAX_BYTES = 200_000;

    private TextView text;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setFitsSystemWindows(true);

        LinearLayout bar = new LinearLayout(this);
        bar.setGravity(Gravity.END);
        bar.addView(button(R.string.diag_refresh, this::refresh));
        bar.addView(button(R.string.diag_copy, this::copy));
        bar.addView(button(R.string.diag_share, this::share));
        root.addView(bar, new LinearLayout.LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT));

        text = new TextView(this);
        text.setTypeface(Typeface.MONOSPACE);
        text.setTextSize(11);
        text.setTextIsSelectable(true);
        int pad = (int) (8 * getResources().getDisplayMetrics().density);
        text.setPadding(pad, pad, pad, pad);
        ScrollView scroll = new ScrollView(this);
        scroll.addView(text);
        root.addView(scroll, new LinearLayout.LayoutParams(LayoutParams.MATCH_PARENT, 0, 1f));
        setContentView(root);
        refresh();
    }

    @Override
    protected void onResume() {
        super.onResume();
        refresh();
    }

    private Button button(int label, Runnable action) {
        Button b = new Button(this);
        b.setText(label);
        b.setOnClickListener(v -> action.run());
        return b;
    }

    private void refresh() {
        text.setText(report());
    }

    private void copy() {
        ClipboardManager clipboard = (ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
        clipboard.setPrimaryClip(ClipData.newPlainText("DexNotas", report()));
        Toast.makeText(this, R.string.diag_copied, Toast.LENGTH_SHORT).show();
    }

    private void share() {
        Intent send = new Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, report());
        startActivity(Intent.createChooser(send, getString(R.string.diag_share)));
    }

    private String report() {
        File files = getFilesDir();
        DisplayMetrics m = getResources().getDisplayMetrics();
        StringBuilder out = new StringBuilder();
        out.append("DexNotas ").append(BuildConfig.VERSION_NAME).append(" (").append(BuildConfig.VERSION_CODE).append(")\n");
        out.append(Build.MANUFACTURER).append(' ').append(Build.MODEL).append(", Android ").append(Build.VERSION.RELEASE)
            .append(" (API ").append(Build.VERSION.SDK_INT).append(")");
        if (Build.VERSION.SDK_INT >= 31) out.append(", ").append(Build.SOC_MODEL);
        out.append('\n');
        out.append("display ").append(m.widthPixels).append('x').append(m.heightPixels).append(" density ").append(m.density).append('\n');
        section(out, "this run (dexnotas.log)", new File(files, "dexnotas.log"));
        section(out, "previous run (dexnotas.prev.log)", new File(files, "dexnotas.prev.log"));
        return out.toString();
    }

    private static void section(StringBuilder out, String title, File file) {
        out.append("\n===== ").append(title).append(" =====\n");
        if (!file.isFile()) {
            out.append("(no file)\n");
            return;
        }
        try (RandomAccessFile in = new RandomAccessFile(file, "r")) {
            long length = in.length();
            long start = Math.max(0, length - MAX_BYTES);
            if (start > 0) out.append("(…).\n");
            in.seek(start);
            byte[] bytes = new byte[(int) (length - start)];
            in.readFully(bytes);
            out.append(new String(bytes, StandardCharsets.UTF_8));
            if (bytes.length > 0 && bytes[bytes.length - 1] != '\n') out.append('\n');
        } catch (Exception e) {
            out.append("cannot read: ").append(e).append('\n');
        }
    }
}
