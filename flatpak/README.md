`com.dcnick3.Shin.json` is a flatpak manifest for building shin with umineko data.

It uses `generated-sources.json` to list sources of cargo dependencies (as this is what flatpak wants).

You should update it every time `Cargo.lock` file changes with `flatpak-cargo-generator ../Cargo.lock -o cargo-sources.json`

Note that for some bespoke reason flatpak-builder can't cache your build if you are providing it as a directory, so I am providing it as a git repo (meaning: you must commit all changes to the repo before building).

---

Even though currently this builds one flatpak with umineko data, probably we want to have a base flatpak with the engine and then have a separate flatpak for each game. This would potentially allow us to distribute the engine in flathub and the game let the user generate user the data flatpak or smth.