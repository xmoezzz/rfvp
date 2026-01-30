package com.rfvp.launcher;

import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.TextView;

import androidx.annotation.NonNull;
import androidx.recyclerview.widget.RecyclerView;

import java.util.ArrayList;
import java.util.List;

public final class GameAdapter extends RecyclerView.Adapter<GameAdapter.Holder> {

    public interface Listener {
        void onGameClicked(GameEntry e);
        void onGameLongPressed(GameEntry e);
    }

    private final Listener listener;
    private final List<GameEntry> items = new ArrayList<>();

    public GameAdapter(Listener listener) {
        this.listener = listener;
    }

    public void setItems(List<GameEntry> newItems) {
        items.clear();
        if (newItems != null) {
            items.addAll(newItems);
        }
        notifyDataSetChanged();
    }

    @NonNull
    @Override
    public Holder onCreateViewHolder(@NonNull ViewGroup parent, int viewType) {
        View v = LayoutInflater.from(parent.getContext()).inflate(R.layout.item_game, parent, false);
        return new Holder(v);
    }

    @Override
    public void onBindViewHolder(@NonNull Holder holder, int position) {
        GameEntry e = items.get(position);
        holder.title.setText(e.title);
        holder.nls.setText(e.nls);
        holder.itemView.setOnClickListener(v -> listener.onGameClicked(e));
        holder.itemView.setOnLongClickListener(v -> {
            listener.onGameLongPressed(e);
            return true;
        });
    }

    @Override
    public int getItemCount() {
        return items.size();
    }

    static final class Holder extends RecyclerView.ViewHolder {
        final TextView title;
        final TextView nls;
        Holder(@NonNull View itemView) {
            super(itemView);
            title = itemView.findViewById(R.id.txt_title);
            nls = itemView.findViewById(R.id.txt_nls);
        }
    }
}
