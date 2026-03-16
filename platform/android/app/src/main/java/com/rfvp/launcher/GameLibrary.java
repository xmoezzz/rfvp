package com.rfvp.launcher;

import android.content.Context;
import android.net.Uri;
import android.os.Environment;
import android.provider.DocumentsContract;

import androidx.annotation.Nullable;
import androidx.documentfile.provider.DocumentFile;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

public final class GameLibrary {

    private final Context ctx;
    private final File libraryFile;

    // For cleanup if import fails
    private volatile File lastPartialDir = null;

    public GameLibrary(Context ctx) {
        this.ctx = ctx.getApplicationContext();
        File base = new File(this.ctx.getFilesDir(), "RFVPLauncher");
        //noinspection ResultOfMethodCallIgnored
        base.mkdirs();
        this.libraryFile = new File(base, "library.json");
    }

    public List<GameEntry> load() {
        try {
            if (!libraryFile.isFile()) {
                return new ArrayList<>();
            }
            byte[] data;
            try (InputStream in = new FileInputStream(libraryFile)) {
                java.io.ByteArrayOutputStream baos = new java.io.ByteArrayOutputStream();
                byte[] buf = new byte[8192];
                int n;
                while ((n = in.read(buf)) >= 0) {
                    if (n > 0) baos.write(buf, 0, n);
                }
                data = baos.toByteArray();
            }
            String s = new String(data, StandardCharsets.UTF_8);
            JSONArray arr = new JSONArray(s);
            List<GameEntry> out = new ArrayList<>();
            for (int i = 0; i < arr.length(); i++) {
                JSONObject o = arr.getJSONObject(i);
                out.add(new GameEntry(
                        o.optString("id"),
                        o.optString("title", "Untitled"),
                        o.optString("rootPath"),
                        o.optString("nls", "sjis"),
                        o.optLong("addedAt", 0L)
                ));
            }
            return out;
        } catch (Throwable t) {
            return new ArrayList<>();
        }
    }

    private void save(List<GameEntry> entries) throws Exception {
        JSONArray arr = new JSONArray();
        for (GameEntry e : entries) {
            JSONObject o = new JSONObject();
            o.put("id", e.id);
            o.put("title", e.title);
            o.put("rootPath", e.rootPath);
            o.put("nls", e.nls);
            o.put("addedAt", e.addedAtEpochMs);
            arr.put(o);
        }
        byte[] data = arr.toString(2).getBytes(StandardCharsets.UTF_8);
        try (OutputStream out = new FileOutputStream(libraryFile, false)) {
            out.write(data);
        }
    }

    /**
     * Import the selected folder in no-copy mode.
     *
     * We only record a directly accessible filesystem path and do not duplicate data.
     *
     * The caller must decide NLS and then call {@link #addImportedGame(ImportedGameDraft, String)}.
     */
    public ImportedGameDraft importFromTreeUri(Uri treeUri) throws Exception {
        DocumentFile tree = DocumentFile.fromTreeUri(ctx, treeUri);
        if (tree == null || !tree.isDirectory()) {
            throw new IllegalArgumentException("Selected URI is not a directory");
        }

        // No-copy mode: SAF tree must be mappable to a directly accessible path.
        String directRootPath = tryResolveDirectRootPath(treeUri);
        if (directRootPath == null || directRootPath.isEmpty()) {
            throw new IllegalStateException("Selected folder provider is not supported in no-copy mode. Please select a local storage folder.");
        }

        File directRoot = new File(directRootPath);
        if (!directRoot.isDirectory()) {
            throw new IllegalStateException("Selected folder path is not accessible: " + directRootPath);
        }

        File hcb = findFirstHcb(directRoot);
        if (hcb == null) {
            throw new IllegalStateException("No .hcb found in selected folder");
        }

        String title = HcbTitleReader.readTitle(hcb);
        if (title == null || title.trim().isEmpty()) {
            title = "Untitled";
        }

        return new ImportedGameDraft(
                UUID.randomUUID().toString(),
                title,
                directRoot.getAbsolutePath(),
                System.currentTimeMillis()
        );
    }

    @Nullable
    private static String tryResolveDirectRootPath(Uri treeUri) {
        if (treeUri == null) return null;
        if (!"content".equalsIgnoreCase(treeUri.getScheme())) return null;

        try {
            String authority = treeUri.getAuthority();
            if (!"com.android.externalstorage.documents".equals(authority)) {
                return null;
            }

            String docId = DocumentsContract.getTreeDocumentId(treeUri);
            if (docId == null || docId.isEmpty()) return null;

            String[] parts = docId.split(":", 2);
            if (parts.length < 1) return null;

            String volume = parts[0];
            String relative = parts.length > 1 ? parts[1] : "";

            File base;
            if ("primary".equalsIgnoreCase(volume)) {
                base = Environment.getExternalStorageDirectory();
            } else {
                // Common pattern for removable storage volume names.
                base = new File("/storage", volume);
            }

            if (base == null) return null;

            if (relative == null || relative.isEmpty()) {
                return base.getAbsolutePath();
            }
            return new File(base, relative).getAbsolutePath();
        } catch (Throwable ignored) {
            return null;
        }
    }

    public GameEntry addImportedGame(ImportedGameDraft draft, String nls) throws Exception {
        if (draft == null) {
            throw new IllegalArgumentException("draft is null");
        }
        if (nls == null || nls.trim().isEmpty()) {
            nls = "sjis";
        }

        GameEntry entry = new GameEntry(draft.id, draft.title, draft.rootPath, nls, draft.addedAtEpochMs);

        List<GameEntry> all = load();
        all.add(0, entry);
        save(all);
        return entry;
    }

    public void updateGameNls(String id, String newNls) throws Exception {
        if (id == null || id.isEmpty()) return;
        if (newNls == null || newNls.trim().isEmpty()) newNls = "sjis";

        List<GameEntry> all = load();
        List<GameEntry> out = new ArrayList<>(all.size());
        for (GameEntry e : all) {
            if (e != null && id.equals(e.id)) {
                out.add(new GameEntry(e.id, e.title, e.rootPath, newNls, e.addedAtEpochMs));
            } else {
                out.add(e);
            }
        }
        save(out);
    }

    /** A staged import result that has not been added to the library yet. */
    public static final class ImportedGameDraft {
        public final String id;
        public final String title;
        public final String rootPath;
        public final long addedAtEpochMs;

        public ImportedGameDraft(String id, String title, String rootPath, long addedAtEpochMs) {
            this.id = id;
            this.title = title;
            this.rootPath = rootPath;
            this.addedAtEpochMs = addedAtEpochMs;
        }
    }

    public void cleanupPartialImport() {
        File dir = lastPartialDir;
        if (dir != null) {
            deleteRecursively(dir);
            lastPartialDir = null;
        }
    }

    private void copyTree(DocumentFile src, File dst) throws Exception {
        if (src.isDirectory()) {
            if (!dst.exists()) {
                //noinspection ResultOfMethodCallIgnored
                dst.mkdirs();
            }
            DocumentFile[] children = src.listFiles();
            if (children == null) return;
            for (DocumentFile child : children) {
                String name = child.getName();
                if (name == null) continue;
                File childDst = new File(dst, name);
                copyTree(child, childDst);
            }
        } else {
            try (InputStream in = ctx.getContentResolver().openInputStream(src.getUri());
                 OutputStream out = new FileOutputStream(dst)) {
                if (in == null) {
                    throw new IllegalStateException("Failed to open: " + src.getUri());
                }
                byte[] buf = new byte[1024 * 256];
                int n;
                while ((n = in.read(buf)) >= 0) {
                    if (n > 0) out.write(buf, 0, n);
                }
            }
        }
    }

    @Nullable
    private static File findFirstHcb(File root) {
        if (root == null || !root.exists()) return null;
        if (root.isFile()) {
            String n = root.getName().toLowerCase();
            if (n.endsWith(".hcb")) return root;
            return null;
        }
        File[] kids = root.listFiles();
        if (kids == null) return null;
        for (File k : kids) {
            File hit = findFirstHcb(k);
            if (hit != null) return hit;
        }
        return null;
    }

    private static void deleteRecursively(File f) {
        if (f == null || !f.exists()) return;
        if (f.isDirectory()) {
            File[] kids = f.listFiles();
            if (kids != null) {
                for (File k : kids) deleteRecursively(k);
            }
        }
        //noinspection ResultOfMethodCallIgnored
        f.delete();
    }
}
