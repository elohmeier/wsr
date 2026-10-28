"""Decode Wall Street Raider obfuscated text files (.wsr scenario / news / quotes)."""
import sys, pathlib
KEY = [28, 30, 32, 34, 36]
def decode_line(l: bytes) -> bytes:
    if l.startswith(b'@'): return l
    return bytes(c if c <= 0x27 else (c - (KEY[i] if i < 5 else 39)) % 256 for i, c in enumerate(l))
out = pathlib.Path(sys.argv[1]); out.mkdir(exist_ok=True)
for p in map(pathlib.Path, sys.argv[2:]):
    lines = p.read_bytes().split(b'\r\n')
    (out / (p.name + '.txt')).write_bytes(b'\r\n'.join(map(decode_line, lines)))
    print(p.name, len(lines), 'lines')
