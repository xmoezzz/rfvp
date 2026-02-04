package com.rfvp.launcher;

import android.content.Context;
import android.view.Surface;

/**
 * JNI bridge to the Rust host-driven Android API.
 *
 * The C++ JNI shim (lib"rfvp_jni") resolves and calls the exported C ABI symbols from librfvp.so:
 *   rfvp_android_create/step/resize/set_surface/touch/destroy
 */
public final class NativeRfvp {
    static {
        // librfvp.so is the Rust engine.
        System.loadLibrary("rfvp");
        // librfvp_jni.so is the tiny JNI + ANativeWindow bridge.
        System.loadLibrary("rfvp_jni");
    }

    private NativeRfvp() {}

    // ndk-context init: must happen before the Rust engine touches CPAL/Oboe.
    // We keep this in Java so the Activity can pass an Application context.
    private static boolean sAndroidContextInited = false;

    /**
     * Initialize the Android JVM/context bridge for the Rust side.
     * Call this once (it is safe to call multiple times).
     */
    public static synchronized void initAndroidContext(Context ctx) {
        if (sAndroidContextInited) {
            return;
        }
        if (ctx == null) {
            return;
        }
        nativeInitAndroidContext(ctx.getApplicationContext());
        sAndroidContextInited = true;
    }

    private static native void nativeInitAndroidContext(Context appContext);

    /** Create an engine instance bound to the given Surface. Returns 0 on failure. */
    public static native long create(
            Surface surface,
            int widthPx,
            int heightPx,
            double nativeScaleFactor,
            String gameDirUtf8,
            String nlsUtf8
    );

    /** Step one frame. Returns non-zero if the engine requested exit. */
    public static native int step(long handle, int dtMs);

    /** Notify surface size change (physical pixels). */
    public static native void resize(long handle, int widthPx, int heightPx);

    /** Rebind to a new Surface (SurfaceView recreated). */
    public static native void setSurface(long handle, Surface surface, int widthPx, int heightPx);

    /** Inject a single-finger touch event (coordinates in physical pixels). */
    public static native void touch(long handle, int phase, double xPx, double yPx);

    /** Destroy the instance. */
    public static native void destroy(long handle);
}
