# Patched dependencies

## nx

`nx/` is [aarch64-switch-rs/nx](https://github.com/aarch64-switch-rs/nx) at the revision in
`Cargo.lock` (`62a39b6`), used through `[patch]` in the workspace `Cargo.toml`. Changes:

- `src/service.rs`, `new_service_object`: no `sm.detach_client` after `get_service_handle`.
  Ryujinx does not implement `sm:` command 4 (`DetachClient`) and stops the emulator the first
  time a service is opened. On hardware the call changes nothing: the `sm` session is closed right
  after, and the client registration goes with it (libnx never makes the call).
- `src/ipc.rs`, `pop_copy_handle` / `pop_move_handle`: take a handle of the other kind when the
  expected one is missing. Ryujinx returns the buffer event of
  `IHOSBinderDriver::GetNativeHandle` as a move handle, and the strict check failed surface
  creation with 10-236.
- `src/gpu/canvas.rs`, `RGBA8`: `A8B8G8R8` (bytes r, g, b, a in memory), the color format libnx
  pairs with `RGBA_8888`, instead of `R8G8B8A8`, which Ryujinx's compositor rejects. The fields
  are reordered to match, so a pixel built from `r, g, b, a` bytes needs no swizzling.

To move to a newer nx, copy its `src`, `nx-derive` and `Cargo.toml` here and apply the change
again (search for "rfvp patch").
