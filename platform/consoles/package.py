#!/usr/bin/env python3
"""Stage and verify actual console executables before creating release archives."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def checked_copy(source, destination, magic):
    source, destination = Path(source), Path(destination)
    with source.open('rb') as stream:
        prefix = stream.read(max(64, len(magic)))
    if not prefix.startswith(magic):
        raise ValueError(f'Invalid executable header: {source}')
    if magic == b'\x7fELF' and (prefix[4:6] != b'\x01\x01' or struct.unpack_from('<H', prefix, 18)[0] != 8):
        raise ValueError(f'Expected a little-endian ELF32 MIPS executable: {source}')
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)


def package(platform, work, output):
    work, output = Path(work).resolve(), Path(output).resolve()
    target = work.parent
    stage = work / 'stage'
    if stage.exists(): shutil.rmtree(stage)
    stage.mkdir()
    if platform == 'ps1':
        checked_copy(work / 'game/RFVP.EXE', stage / 'rfvp-ps1.exe', b'PS-X EXE')
        binary = work / 'game/rfvp.bin'
        if binary.stat().st_size == 0 or binary.stat().st_size % 2352:
            raise ValueError('PS1 image must contain whole raw CD sectors')
        cue = (work / 'game/rfvp.cue').read_text()
        if 'rfvp.bin' not in cue or 'MODE2/2352' not in cue:
            raise ValueError('PS1 CUE must reference the packaged raw disc image')
        for name in ['rfvp.bin', 'rfvp.cue']: shutil.copy2(work / 'game' / name, stage / name)
        shutil.copytree(work / 'game', stage / 'disc-files', ignore=shutil.ignore_patterns('rfvp.bin', 'rfvp.cue'))
        instructions = 'Boot rfvp.cue (keep rfvp.bin beside it). The disc contains the original RFVP demo.\nTo use your own game, convert it with rfvp-psx-convert, copy rfvp-ps1.exe as\nRFVP.EXE into its converted directory, then run mkpsxiso -y iso.xml there.\nThe runtime targets stock 2 MiB RAM. Disc files are included for repacking.\n'
    elif platform == 'ps2':
        checked_copy(target / 'ps2/rfvp-ps2.elf', stage / 'rfvp-ps2.elf', b'\x7fELF')
        iso = work / 'rfvp-ps2.iso'
        with iso.open('rb') as stream:
            stream.seek(16 * 2048)
            if stream.read(7) != b'\x01CD001\x01': raise ValueError('Invalid ISO9660 primary volume descriptor')
        subprocess.run(['xorriso', '-indev', str(iso), '-find', '/', '-type', 'f', '-exec', 'lsdl'], check=True)
        shutil.copy2(iso, stage / iso.name)
        shutil.copytree(work / 'game', stage / 'game')
        instructions = 'Boot rfvp-ps2.iso to run the original RFVP demo.\nFor host-file loading in PCSX2, enable Host Filesystem and boot rfvp-ps2.elf.\nReplace the sibling game directory with rfvp-rebuilder --platform ps2 output.\nFor USB loading, put the converted game in mass:/RFVP. Optical disc saves use mc0:/RFVP.\n'
    elif platform == 'psp':
        candidates = sorted((target / 'mipsel-sony-psp/release').rglob('*.PBP'))
        candidates += sorted((target / 'mipsel-sony-psp/release').rglob('*.pbp'))
        if len(candidates) != 1: raise ValueError(f'Expected one PSP EBOOT, found {candidates}')
        install = stage / 'PSP/GAME/RFVP'
        shutil.copytree(work / 'game', install)
        checked_copy(candidates[0], install / 'EBOOT.PBP', b'\0PBP')
        instructions = 'Copy PSP/GAME/RFVP to the memory stick and launch it through homebrew.\nThe original RFVP demo is included. Replace the files beside EBOOT.PBP with\nrfvp-rebuilder --platform psp output for your own game. Keep EBOOT.PBP.\n'
    elif platform == '3ds':
        install = stage / '3ds/rfvp'
        shutil.copytree(work / 'game', install / 'game')
        checked_copy(ROOT / 'target/rfvp-3ds/3ds/rfvp.3dsx', install / 'rfvp.3dsx', b'3DSX')
        instructions = 'Copy 3ds/rfvp to the SD card and launch rfvp.3dsx in Homebrew Launcher.\nThe original RFVP demo is included. Replace 3ds/rfvp/game with\nrfvp-rebuilder --platform 3ds output for your own game.\nReal hardware needs your own sdmc:/3ds/dspfirm.cdc for sound.\n'
    else: raise ValueError(f'Unknown console: {platform}')
    version = tomllib.loads((ROOT / 'crates/rfvp/Cargo.toml').read_text())['package']['version']
    commit = os.environ.get('RFVP_BUILD_COMMIT') or subprocess.check_output(
        ['git', '-c', f'safe.directory={ROOT}', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    (stage / 'README.txt').write_text(f'RFVP {version} - {platform.upper()}\nCommit: {commit}\n\n' + instructions)
    shutil.copy2(ROOT / 'LICENSE', stage / 'LICENSE')
    (stage / 'build.json').write_text(json.dumps(dict(platform=platform, version=version, commit=commit,
                                                    content='Original RFVP demo; no commercial game assets'), indent=2) + '\n')
    hashes = [f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.relative_to(stage).as_posix()}'
              for p in sorted(stage.rglob('*')) if p.is_file()]
    (stage / 'SHA256SUMS').write_text('\n'.join(hashes) + '\n')
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f'rfvp-{platform}.zip'
    with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED) as z:
        for path in sorted(stage.rglob('*')):
            if path.is_file(): z.write(path, path.relative_to(stage).as_posix())
    with zipfile.ZipFile(archive) as z:
        if z.testzip() is not None: raise ValueError(f'Corrupt archive: {archive}')
    print(archive)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('platform', choices=['ps1', 'ps2', 'psp', '3ds'])
    parser.add_argument('work', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    package(args.platform, args.work, args.output)
