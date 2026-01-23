package com.rfvp.launcher;

import com.google.androidgamesdk.GameActivity;

public final class RfvpGameActivity extends GameActivity {
    static {
        System.loadLibrary("rfvp");
    }
}
