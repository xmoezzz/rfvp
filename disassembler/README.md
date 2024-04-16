# FVP disassembler
A Non-offical disassembler for the FVP Engine.

## Usage
```bash
$ ./disassembler

  --input <INPUT>
  --output <OUTPUT>

Usage: disassembler --input <INPUT> --output <OUTPUT>
```
* input: Path to the FVP binary, usually ending with `.bin`
* output: The output path, FVP binary will be disassembled to this path
* nls: Codepage, the default value is sjis(Shift_JIS), available values are: sjis, utf8, gbk

### Project layout
```
path_dir
├── config.yaml (project configuration file, the only value you can edit is "game_title.")
├── disassembly.yaml (disassembled file, typically, you achieve translation by modifying the value corresponding to push_string.)
├── project.toml (do not edit)
```

## How to build
```bash
cargo build --release -p disassembler
```

## supported platforms
- Windows
- Linux
- MacOS
- etc...

## Warning
* The tool is generally used for game translation purposes and does not provide SDK-level editing capabilities.
* This tool is unofficial, and the FVP engine is not authorized for the 3rd-party game development.
* Please do not use this tool for re-editing or for re-creation of office games.
* This is a part of the rfvp project, and the The rfvp project is developed based on independent reverse engineering of the original engine. All rfvp code is unrelated to the original engine code.