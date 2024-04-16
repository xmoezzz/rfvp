# FVP assembler
A Non-offical assembler for the FVP Engine.

## Usage
```bash
$ ./assembler

the following required arguments were not provided:
  --project-dir <PROJECT_DIR>
  --output <OUTPUT>
  --nls <NLS>

Usage: assembler --project-dir <PROJECT_DIR> --output <OUTPUT> --nls <NLS>
```
* project-dir: Path to the FVP project, which was created by the disassembler
* output: The output path, FVP binary will be generated to here
* nls: Codepage, the default value is sjis(Shift_JIS), available values are: sjis, utf8, gbk
    * ⚠️The original FVP engine only supports Shift_JIS, so please use this option carefully.
    * ⚠️If you use utf8 or gbk, please make some patch to the FVP engine.
    * ⚠️For English translation, both GBK and SJIS encoding are sufficient.


## How to build
```bash
cargo build --release -p assembler
```

## supported platforms
- Windows
- Linux
- MacOS
- etc...

## Warning
* The tool is generally used for game translation purposes and does not provide SDK-level editing capabilities.
* This tool is unofficial, and the FVP engine is not authorized for the 3rd-party game development.
* 💣💣💣This assembler not only restores translated strings back into the file, but also performs **complete instruction assembly operations**. If you're not familiar with FVP instructions, please refrain from modifying those instructions and their corresponding parameters casually.
* Please do not use this tool for re-editing or for re-creation of office games.
* This is a part of the rfvp project, and the The rfvp project is developed based on independent reverse engineering of the original engine. All rfvp code is unrelated to the original engine code.

