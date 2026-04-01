# nvsg_pack

Standalone CLI crate for packing and unpacking FAVORITE `HZC1 + NVSG` textures.

## Supported texture types

- `single24`
- `single32`
- `multi32`
- `single8`
- `single1`

## Examples

Pack a premultiplied-alpha 32-bit texture from one PNG:

```bash
cargo run -p nvsg_pack -- pack \
  --type single32 \
  --output ui_button.nvsg \
  ui_button.png
```

Pack a multi-frame parts texture:

```bash
cargo run -p nvsg_pack -- pack \
  --type multi32 \
  --output parts_anim.nvsg \
  frame_000.png frame_001.png frame_002.png
```

Pack an 8-bit mask from alpha:

```bash
cargo run -p nvsg_pack -- pack \
  --type single8 \
  --mask-source alpha \
  --output mask.nvsg \
  mask.png
```

Inspect an existing file:

```bash
cargo run -p nvsg_pack -- inspect texture.nvsg
```

Decode back to PNG:

```bash
cargo run -p nvsg_pack -- unpack texture.nvsg --output texture.png
```
