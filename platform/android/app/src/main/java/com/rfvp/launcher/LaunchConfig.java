package com.rfvp.launcher;

import android.content.Context;

import org.json.JSONObject;

import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;

/**
 * A tiny launch contract between Java wrapper and native code.
 *
 * Native side should read <filesDir>/RFVPLauncher/launch.json at startup and call:
 *   rfvp_run_entry(game_root_utf8, nls_utf8)
 */
public final class LaunchConfig {

    private LaunchConfig() {}

    public static void write(Context ctx, String gameRootUtf8, String nlsUtf8) throws Exception {
        File base = new File(ctx.getFilesDir(), "RFVPLauncher");
        //noinspection ResultOfMethodCallIgnored
        base.mkdirs();

        File f = new File(base, "launch.json");
        JSONObject o = new JSONObject();
        o.put("game_root_utf8", gameRootUtf8);
        o.put("nls_utf8", nlsUtf8);

        byte[] data = o.toString(2).getBytes(StandardCharsets.UTF_8);
        try (FileOutputStream out = new FileOutputStream(f, false)) {
            out.write(data);
        }
    }
}
