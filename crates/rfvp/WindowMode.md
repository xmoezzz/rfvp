# WindowMode

This note documents the WindowMode behavior implemented in this tree. It only states behavior that is backed by reverse-engineering evidence from the original engine and the uploaded sqlite database.

## Confirmed original behavior

The original `WindowMode` syscall accepts integer values from `-1` to `6`.

- `WindowMode(0)` sets internal `render_flag = 0` and returns `0`.
- `WindowMode(1)` sets internal `render_flag = 1` and returns `1`.
- `WindowMode(-1)` returns `1` by default. If exact game-resolution exclusive fullscreen is supported, it sets `render_flag = 2` and returns `-1`.
- `WindowMode(2)` queries the current mode. The visible remap is: internal `2 -> -1`, internal `3 -> -2`, otherwise the raw flag value is returned.
- `WindowMode(3)` queries support for the `-1` mode.
- `WindowMode(4)` sets `is_first_frame = 1`.
- `WindowMode(5)` sets `is_first_frame = 0`.
- `WindowMode(6)` queries `is_first_frame`.

## Focus-loss side effect

`WindowMode(4/5/6)` does not query window activation directly.

The original engine handles activation in `WM_ACTIVATEAPP`. When the engine is in a fullscreen-class render state and `is_first_frame` is enabled, losing activation forces `render_flag = 0` and resets the D3D device.

This behavior is confirmed in the original engine, but it is **not** wired into the shared host focus path in this tree right now. The previous attempt to mirror it there interfered with the settings text sample path, so it was removed.

The current tree therefore only keeps the confirmed `is_first_frame` set / clear / query behavior for `WindowMode(4/5/6)`. It does **not** claim that the focus-loss fallback to `render_flag = 0` has been aligned yet.

## Fullscreen support query

The original engine only enables the `-1` mode when the graphics backend enumerates a display mode whose width and height exactly match the game resolution.

This tree therefore treats `WindowMode(3)` as:

- desktop: true only when an exact `game_w x game_h` video mode exists
- mobile: always supported at the script level, because the host surface is fullscreen

## What is intentionally not claimed here

The original query path can return `-2` when internal `render_flag == 3`. That mapping is present in the original syscall.

However, in the uploaded reverse-engineering database, the normal producer path for `render_flag = 3` was not closed. Because of that, this tree does not assign a special presentation or input-mapping behavior to internal `render_flag == 3`.

The port keeps the query remap `3 -> -2`, but does not claim that `-2` has been semantically aligned beyond that return value.
