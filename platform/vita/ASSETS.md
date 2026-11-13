# PSV bubble and LiveArea artwork

The checked-in `host/static/sce_sys/` images adapt the project's existing
`platform/wasm/icon/icon-512.png` (SHA256
`509275d1c67e0ed9966b5c5d3aaf75d2a7f160839bc63f50a0f3d053b392da58`).
The original artwork is resized and composited over its existing blue background;
no replacement artwork is generated. The existing source attribution/license applies.

- `icon0.png`: 128 × 128, indexed 8-bit PNG, no transparency.
- `livearea/contents/startup.png`: 280 × 158, original icon centered on blue.
- `livearea/contents/bg.png`: 840 × 500, matching blue background.
- `livearea/contents/template.xml`: minimal a1 LiveArea layout, UTF-8/CRLF.

The existing `[package.metadata.vita] assets = "static"` includes these files in
future cargo-vita VPK builds. The layout and indexed image encoding follow the
[VitaSDK sample](https://github.com/vitasdk/samples/tree/master/hello_world/sce_sys)
and [image notes](https://github.com/vitasdk/samples#notes-on-images).

`python3 platform/vita/prepare_assets.py` (Pillow required)
reproduces the assets from the source icon. Normal VPK builds use the committed
PNGs and do not require Pillow.
