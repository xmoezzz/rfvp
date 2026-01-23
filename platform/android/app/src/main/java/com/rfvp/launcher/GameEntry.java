package com.rfvp.launcher;

public final class GameEntry {
    public final String id;
    public final String title;
    public final String rootPath;
    public final String nls;
    public final long addedAtEpochMs;

    public GameEntry(String id, String title, String rootPath, String nls, long addedAtEpochMs) {
        this.id = id;
        this.title = title;
        this.rootPath = rootPath;
        this.nls = nls;
        this.addedAtEpochMs = addedAtEpochMs;
    }
}
