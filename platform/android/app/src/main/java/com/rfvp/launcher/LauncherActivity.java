package com.rfvp.launcher;

import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.widget.Button;
import android.widget.Toast;

import androidx.activity.result.ActivityResultLauncher;
import androidx.activity.result.contract.ActivityResultContracts;
import androidx.annotation.Nullable;
import androidx.appcompat.app.AppCompatActivity;
import androidx.recyclerview.widget.GridLayoutManager;
import androidx.recyclerview.widget.RecyclerView;

import java.io.File;
import java.util.List;

/**
 * Minimal launcher UI:
 * - Shows imported games as a scrollable grid.
 * - Import: select a directory (SAF), then copy into app-private storage so native code can access it via a real path.
 * - Run: taps a tile -> writes launch.json -> starts GameActivity.
 */
public final class LauncherActivity extends AppCompatActivity implements GameAdapter.Listener {

    private GameLibrary library;
    private GameAdapter adapter;

    private final ActivityResultLauncher<Uri> openTreeLauncher =
            registerForActivityResult(new ActivityResultContracts.OpenDocumentTree(), this::onImportTreeSelected);

    @Override
    protected void onCreate(@Nullable Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        setContentView(R.layout.activity_launcher);

        library = new GameLibrary(this);

        RecyclerView rv = findViewById(R.id.game_grid);
        rv.setLayoutManager(new GridLayoutManager(this, 3));
        adapter = new GameAdapter(this);
        rv.setAdapter(adapter);

        Button importBtn = findViewById(R.id.btn_import);
        importBtn.setOnClickListener(v -> openTreeLauncher.launch(null));

        refresh();
    }

    private void refresh() {
        List<GameEntry> entries = library.load();
        adapter.setItems(entries);
    }

    private void onImportTreeSelected(@Nullable Uri treeUri) {
        if (treeUri == null) {
            return;
        }

        // Persist permission so we can re-import / retry if needed.
        final int flags = Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION;
        try {
            getContentResolver().takePersistableUriPermission(treeUri, flags);
        } catch (Throwable ignored) {
            // Some providers may not support persistable perms; import copy will still work for this run.
        }

        Toast.makeText(this, "Importing...", Toast.LENGTH_SHORT).show();

        new Thread(() -> {
            try {
                GameEntry e = library.importFromTreeUri(treeUri);
                runOnUiThread(() -> {
                    Toast.makeText(this, "Imported: " + e.title, Toast.LENGTH_SHORT).show();
                    refresh();
                });
            } catch (Throwable t) {
                library.cleanupPartialImport();
                runOnUiThread(() -> Toast.makeText(this, "Import failed: " + t.getMessage(), Toast.LENGTH_LONG).show());
            }
        }, "rfvp-import").start();
    }

    @Override
    public void onGameClicked(GameEntry e) {
        try {
            // Write launch config that native code can read on startup.
            LaunchConfig.write(this, e.rootPath, e.nls);

            Intent it = new Intent(this, RfvpGameActivity.class);
            startActivity(it);
        } catch (Throwable t) {
            Toast.makeText(this, "Failed to start: " + t.getMessage(), Toast.LENGTH_LONG).show();
        }
    }
}
