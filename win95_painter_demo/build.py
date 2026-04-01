#!/usr/bin/env python3
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
WORKSPACE_ROOT = SCRIPT_DIR.parent
CRATES_DIR = WORKSPACE_ROOT / "crates"
COMPILER_CRATE_DIR = CRATES_DIR / "lua2hcb_compiler"


def find_single_file(base: Path, patterns: list[str], description: str) -> Path:
    matches: list[Path] = []
    for pattern in patterns:
        matches.extend(sorted(base.glob(pattern)))

    files = [p for p in matches if p.is_file()]
    if not files:
        raise SystemExit(f"No {description} file found in {base}")
    if len(files) != 1:
        joined = "\n  ".join(str(p) for p in files)
        raise SystemExit(
            f"Expected exactly one {description} file in {base}, found {len(files)}:\n  {joined}"
        )
    return files[0]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build the current Luax demo into an HCB file using lua2hcb_compiler."
    )
    parser.add_argument("--meta", type=Path, help="Path to the project YAML file")
    parser.add_argument("--lua", type=Path, help="Path to the .luax source file")
    parser.add_argument("-o", "--out", type=Path, help="Path to the output .hcb file")
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    if not CRATES_DIR.is_dir():
        raise SystemExit(f"Missing crates directory: {CRATES_DIR}")
    if not COMPILER_CRATE_DIR.is_dir():
        raise SystemExit(f"Missing compiler crate directory: {COMPILER_CRATE_DIR}")

    meta = args.meta.resolve() if args.meta else find_single_file(SCRIPT_DIR, ["*.yaml", "*.yml"], "YAML")
    lua = args.lua.resolve() if args.lua else find_single_file(SCRIPT_DIR, ["*.luax"], "Luax")
    out = args.out.resolve() if args.out else (SCRIPT_DIR / f"{lua.stem}.hcb")

    if not meta.is_file():
        raise SystemExit(f"YAML file does not exist: {meta}")
    if not lua.is_file():
        raise SystemExit(f"Luax file does not exist: {lua}")
    if out.exists() and out.is_dir():
        raise SystemExit(f"Output path is a directory: {out}")

    command = [
        "cargo",
        "run",
        "--release",
        "-p",
        "lua2hcb_compiler",
        "--",
        "--meta",
        str(meta),
        "--lua",
        str(lua),
        "-o",
        str(out),
    ]

    print("Workspace root:", WORKSPACE_ROOT)
    print("Command:")
    print(" ".join(command))

    completed = subprocess.run(command, cwd=WORKSPACE_ROOT)
    if completed.returncode != 0:
        return completed.returncode

    print("Build finished:", out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
