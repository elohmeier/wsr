#!/usr/bin/env python3
"""Check the public snapshot; optionally compare source/builds with private game binaries.

The private scan detects verbatim printable prose, including escaped source strings. It
cannot establish clean-room provenance or detect every paraphrase, compressed resource,
or dataset. Matches are reported as locations and hashes, never as copied game text.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
STRUCTURAL_NAMES = {
    'MSGBOX', 'Corp20', 'Player2', 'FND', 'XYZ', 'ABC', 'ECB', 'Jane',
    'Target Corp', 'ACME Corp', 'JOE', 'Ann', 'GAME01.DAT', 'GAME07.DAT', 'UK', 'M',
}
FORBIDDEN_SUFFIXES = {'.exe', '.dll', '.dat', '.wsr', '.bmp', '.jpg', '.jpeg', '.zip', '.7z'}
TEXT_SUFFIXES = {'.rs', '.py', '.mjs', '.js', '.ts', '.svelte', '.md', '.tsv', '.json', '.txt', '.html'}


def public_files():
    result = subprocess.check_output(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=ROOT
    )
    return sorted({ROOT / p.decode() for p in result.split(b'\0') if p})


def metadata_errors():
    manifest = json.loads((ROOT / 'web/content-manifest.json').read_text())
    errors = []
    if manifest['version'] != 2:
        errors.append('Unsupported extraction manifest version')
    for name, digest in manifest['sources'].items():
        if not re.fullmatch(r'[A-Z0-9_-]+\.(?:EXE|DLL)', name) or not re.fullmatch(r'[0-9a-f]{64}', digest):
            errors.append('Invalid source image metadata')
    identifiers = []
    for entry in manifest['entries']:
        identifiers.append(entry['id'])
        parts = entry.get('parts', [entry])
        if not parts:
            errors.append(f'Empty recipe: {entry["id"]}')
        for part in parts:
            if 'token' in part:
                token = part['token']
                if not (token in STRUCTURAL_NAMES or re.fullmatch(r'[^A-Za-z]*', token)
                        or re.fullmatch(r'%%[A-Z_]+%%', token)):
                    errors.append(f'Nonstructural literal token: {entry["id"]}')
            elif (part.get('source') not in manifest['sources'] or
                  type(part.get('offset')) is not int or part['offset'] < 0 or
                  type(part.get('length')) is not int or not 1 <= part['length'] <= 65536):
                errors.append(f'Invalid extraction range: {entry["id"]}')
    if identifiers != sorted(set(identifiers)):
        errors.append('Resource identifiers must be unique and sorted')
    from generate_content_schema import generate
    if (ROOT / 'engine/wsr-core/src/content_ids.rs').read_text() != generate():
        errors.append('Generated Rust resource schema differs from the manifest')
    return errors


def reference_windows(directory, size=32):
    windows = set()
    files = [p for p in directory.iterdir() if p.suffix.lower() in {'.exe', '.dll'}]
    if not files:
        raise ValueError('No private reference binaries found')
    for file in files:
        for run in re.finditer(rb'[\x20-\x7e]{32,}', file.read_bytes()):
            text = run.group()
            for start in range(len(text) - size + 1):
                value = text[start:start + size]
                if len(re.findall(rb'[A-Za-z]{2,}', value)) >= 4:
                    windows.add(value)
    return windows


def prose_matches(path, windows, size=32):
    data = path.read_bytes()
    variants = [data]
    if path.suffix in TEXT_SUFFIXES:
        try:
            text = data.decode('utf8')
            text = re.sub(r'\\(?:r|n|t|"|\\)', lambda m: {
                '\\r': '\r', '\\n': '\n', '\\t': '\t', '\\"': '"', '\\\\': '\\'
            }[m.group()], text)
            variants.append(text.encode('cp1252', errors='replace'))
        except UnicodeDecodeError:
            pass
    matches = set()
    for data in variants:
        for offset in range(len(data) - size + 1):
            window = data[offset:offset + size]
            if window in windows:
                matches.add((data[:offset].count(b'\n') + 1 if path.suffix in TEXT_SUFFIXES else offset,
                             hashlib.sha256(window).hexdigest()[:16]))
    return sorted(matches)


def scan_paths(paths):
    for path in paths:
        if path.is_dir():
            yield from (p for p in path.rglob('*') if p.is_file())
        elif path.is_file():
            yield path
        else:
            raise ValueError(f'Scan path does not exist: {path}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--original-dir', type=Path)
    parser.add_argument('--artifacts', type=Path, action='append', default=[])
    parser.add_argument('--scan-path', type=Path, action='append', default=[],
                        help='Also scan an explicit file/directory, e.g. a private negative control')
    args = parser.parse_args()
    errors = metadata_errors()
    files = [p for p in public_files() if p.is_file()]
    for path in files:
        if path.suffix.lower() in FORBIDDEN_SUFFIXES or path.read_bytes()[:7] == b'WSRCP00':
            errors.append(f'Bundled resource file: {path.relative_to(ROOT)}')
    artifacts = list(scan_paths(args.artifacts))
    for path in artifacts:
        if path.suffix.lower() in FORBIDDEN_SUFFIXES or path.read_bytes()[:7] == b'WSRCP00':
            errors.append(f'Bundled resource artifact: {path}')
    if args.original_dir:
        windows = reference_windows(args.original_dir)
        targets = [p for p in files if p.suffix in TEXT_SUFFIXES
                   and p.name not in {'content-manifest.json', 'package-lock.json'}]
        for path in dict.fromkeys(targets + artifacts + list(scan_paths(args.scan_path))):
            matches = prose_matches(path, windows)
            if matches:
                # One finding per file keeps private reports bounded and avoids quoting prose.
                location, digest = matches[0]
                errors.append(f'Verbatim prose candidate: {path}:{location} fingerprint={digest}')
        print(f'Compared {len(targets) + len(artifacts)} source/artifact files with private references.')
    elif args.scan_path:
        parser.error('--scan-path requires --original-dir')
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        return 1
    print(f'Content audit passed ({len(files)} public files, {len(artifacts)} artifacts).')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
