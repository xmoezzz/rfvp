# Translation Tooling and Packaging

This directory hosts scripts, utilities, and workflows for preparing and packaging localized visual novel assets for rfvp.

## Architecture Guidelines (ADR-0008)

As documented in [ADR-0008](../docs/adr/0008-translation-patching-offline-repack.md), rfvp deliberately avoids runtime translation probing or dynamic string hooking in the engine core.

- **Offline Repack:** All translation patches must be prepared offline by repacking translated scripts back into standard `.hcb` bytecode files (using `crates/hcb2lua_decompiler`, `crates/lua2hcb_compiler`, and related rebuilder utilities).
- **No Runtime Probing:** By packaging translations into `.hcb` scripts (and `.bin` archives where necessary), the game runs with zero filesystem probing overhead, eliminating disk latency and debug noise on console targets such as PlayStation Vita and Nintendo Switch.
- **Booting Translations:** Boot translated scripts by configuring `config.toml` in the game directory:
  ```toml
  [game]
  hcb = "YourTranslatedScript.hcb"
  nls = "gbk" # or "utf8", "sjis"
  ```

Please contribute title-specific or general extraction and repacking scripts to this directory.
