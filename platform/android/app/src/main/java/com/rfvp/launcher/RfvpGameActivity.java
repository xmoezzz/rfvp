package com.rfvp.launcher;

import android.os.Bundle;
import android.util.DisplayMetrics;
import android.view.Choreographer;
import android.view.MotionEvent;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.widget.Toast;

import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import androidx.appcompat.app.AppCompatActivity;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import androidx.core.view.WindowInsetsControllerCompat;

import org.json.JSONObject;

import java.io.File;
import java.io.FileInputStream;
import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;

/**
 * Android player activity using the same host-driven model as iOS:
 * - Java owns the main loop (Choreographer)
 * - Java owns the Surface lifecycle (SurfaceView)
 * - Rust is stepped via rfvp_android_* exported symbols
 */
public final class RfvpGameActivity extends AppCompatActivity
        implements SurfaceHolder.Callback, Choreographer.FrameCallback, View.OnTouchListener {

    private SurfaceView surfaceView;

    private long handle = 0;
    private boolean running = false;
    private long lastFrameNs = 0;

    private String gameRoot;
    private String nls;

    @Override
    protected void onCreate(@Nullable Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        // Fullscreen immersive.
        WindowCompat.setDecorFitsSystemWindows(getWindow(), false);
        setContentView(R.layout.activity_player);

        surfaceView = findViewById(R.id.surface_view);
        LaunchParams p = readLaunchParams();
        if (p == null) {
            Toast.makeText(this, "Missing launch.json", Toast.LENGTH_LONG).show();
            finish();
            return;
        }
        gameRoot = p.gameRoot;
        nls = p.nls;

        surfaceView.getHolder().addCallback(this);
        surfaceView.setOnTouchListener(this);
        surfaceView.setFocusable(true);
        surfaceView.setFocusableInTouchMode(true);
        surfaceView.requestFocus();
        surfaceView.setKeepScreenOn(true);

        applyImmersive();
    }

    @Override
    protected void onResume() {
        super.onResume();
        applyImmersive();
        maybeStartFrameLoop();
    }

    @Override
    protected void onPause() {
        stopFrameLoop();
        super.onPause();
    }

    @Override
    protected void onDestroy() {
        stopFrameLoop();
        destroyEngine();
        super.onDestroy();
    }

    private void applyImmersive() {
        final View decor = getWindow().getDecorView();
        WindowInsetsControllerCompat c = ViewCompat.getWindowInsetsController(decor);
        if (c != null) {
            c.hide(WindowInsetsCompat.Type.systemBars());
            c.setSystemBarsBehavior(WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
        }
    }

    // ---- Surface lifecycle ----

    @Override
    public void surfaceCreated(@NonNull SurfaceHolder holder) {
        ensureEngine(holder);
        maybeStartFrameLoop();
    }

    @Override
    public void surfaceChanged(@NonNull SurfaceHolder holder, int format, int width, int height) {
        if (handle == 0) {
            ensureEngine(holder);
        } else {
            // SurfaceChanged fires frequently (format/size). Avoid recreating the WGPU surface here;
            // just resize the existing swapchain. Surface recreation is handled by surfaceCreated/Destroyed.
            NativeRfvp.resize(handle, width, height);
        }
    }

    @Override
    public void surfaceDestroyed(@NonNull SurfaceHolder holder) {
        // The ANativeWindow behind this Surface is about to become invalid.
        stopFrameLoop();
        destroyEngine();
        finish();
    }

    private void ensureEngine(@NonNull SurfaceHolder holder) {
        if (handle != 0) {
            return;
        }
        if (gameRoot == null || gameRoot.isEmpty()) {
            Toast.makeText(this, "Missing game root", Toast.LENGTH_LONG).show();
            finish();
            return;
        }

        DisplayMetrics dm = getResources().getDisplayMetrics();
        double scale = dm.density;

        // Must initialize ndk-context before the Rust engine initializes audio backends.
        NativeRfvp.initAndroidContext(getApplicationContext());

        int w = holder.getSurfaceFrame() != null ? holder.getSurfaceFrame().width() : surfaceView.getWidth();
        int h = holder.getSurfaceFrame() != null ? holder.getSurfaceFrame().height() : surfaceView.getHeight();
        if (w <= 0 || h <= 0) {
            w = Math.max(1, surfaceView.getWidth());
            h = Math.max(1, surfaceView.getHeight());
        }

        long hnd = NativeRfvp.create(holder.getSurface(), w, h, scale, gameRoot, nls);
        if (hnd == 0) {
            Toast.makeText(this, "Failed to create engine", Toast.LENGTH_LONG).show();
            finish();
            return;
        }
        handle = hnd;
    }

    private void destroyEngine() {
        if (handle != 0) {
            NativeRfvp.destroy(handle);
            handle = 0;
        }
    }

    // ---- Frame loop ----

    private void maybeStartFrameLoop() {
        if (!running && handle != 0) {
            running = true;
            lastFrameNs = 0;
            Choreographer.getInstance().postFrameCallback(this);
        }
    }

    private void stopFrameLoop() {
        if (running) {
            running = false;
            lastFrameNs = 0;
            Choreographer.getInstance().removeFrameCallback(this);
        }
    }

    @Override
    public void doFrame(long frameTimeNanos) {
        if (!running || handle == 0) {
            return;
        }

        if (lastFrameNs == 0) {
            lastFrameNs = frameTimeNanos;
        }
        long dtNs = frameTimeNanos - lastFrameNs;
        lastFrameNs = frameTimeNanos;

        int dtMs = (int) (dtNs / 1_000_000L);
        if (dtMs < 0) dtMs = 0;
        if (dtMs > 250) dtMs = 250; // clamp (pause/background)

        int exit = NativeRfvp.step(handle, dtMs);
        if (exit != 0) {
            finish();
            return;
        }

        Choreographer.getInstance().postFrameCallback(this);
    }

    // ---- Touch ----

    @Override
    public boolean onTouch(View v, MotionEvent e) {
        if (handle == 0 || e == null) {
            return false;
        }

        int action = e.getActionMasked();
        int phase;
        switch (action) {
            case MotionEvent.ACTION_DOWN:
                phase = 0;
                break;
            case MotionEvent.ACTION_MOVE:
                phase = 1;
                break;
            case MotionEvent.ACTION_UP:
                phase = 2;
                break;
            case MotionEvent.ACTION_CANCEL:
                phase = 3;
                break;
            default:
                return false;
        }

        double x = e.getX();
        double y = e.getY();
        NativeRfvp.touch(handle, phase, x, y);
        return true;
    }

    // ---- Launch contract ----

    private static final class LaunchParams {
        final String gameRoot;
        final String nls;

        LaunchParams(String gameRoot, String nls) {
            this.gameRoot = gameRoot;
            this.nls = nls;
        }
    }

    @Nullable
    private LaunchParams readLaunchParams() {
        try {
            File base = new File(getFilesDir(), "RFVPLauncher");
            File f = new File(base, "launch.json");
            if (!f.isFile()) {
                return null;
            }
            byte[] data;
            try (FileInputStream in = new FileInputStream(f);
                 ByteArrayOutputStream out = new ByteArrayOutputStream()) {
                byte[] buf = new byte[8192];
                int n;
                while ((n = in.read(buf)) >= 0) {
                    if (n > 0) {
                        out.write(buf, 0, n);
                    }
                }
                data = out.toByteArray();
            }
            String s = new String(data, StandardCharsets.UTF_8);
            JSONObject o = new JSONObject(s);
            String root = o.optString("game_root_utf8", "");
            String nls = o.optString("nls_utf8", "sjis");
            if (root == null || root.isEmpty()) {
                return null;
            }
            return new LaunchParams(root, nls);
        } catch (Throwable t) {
            return null;
        }
    }
}
