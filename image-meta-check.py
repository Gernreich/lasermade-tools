#!/usr/bin/env python3
"""Does any tracked image carry metadata?

    python3 image-meta-check.py REPO [REPO ...]

Every photograph here comes off a phone, and a phone writes itself into the
file. On 2026-10-14 the published build photos were found carrying the camera,
the time, its settings, a Display P3 colour profile and a GPS block -- one that
held only the compass bearing, not a position, though a phone set otherwise
writes the position there too. The author's rule since then is that no image
carries any metadata at all. This holds every image `git ls-files` lists to that rule.

It reads the file's own structure rather than asking a library what it found,
so nothing is missed for being a tag the library does not name:

  JPEG   any APPn segment but APP0 (the JFIF header the format needs) -- APP1 is
         EXIF and XMP, APP2 the ICC profile, APP13 IPTC -- and any COM comment
  PNG    tEXt, zTXt, iTXt, eXIf, iCCP, pHYs and tIME chunks

A colour profile counts. Stripping one from a P3 photo would wash it out, so
the fix is to convert the pixels to sRGB first and then save with no profile;
the commit that brought this check in did exactly that to every photo.

Exit status is 1 if any image fails, so it can gate a commit.
"""
import os
import struct
import subprocess
import sys

PNG_SIG = b'\x89PNG\r\n\x1a\n'
PNG_META = {b'tEXt', b'zTXt', b'iTXt', b'eXIf', b'iCCP', b'pHYs', b'tIME'}
JPEG_NAMES = {0xE1: 'EXIF/XMP', 0xE2: 'ICC profile', 0xED: 'IPTC', 0xFE: 'comment'}


def jpeg_meta(b):
    found, i = [], 2
    while i + 4 <= len(b):
        if b[i] != 0xFF:
            break
        m = b[i + 1]
        if m == 0xD9 or m == 0xDA:           # end of image, or the scan: no more headers
            break
        if 0xD0 <= m <= 0xD7 or m == 0x01:
            i += 2
            continue
        n = struct.unpack('>H', b[i + 2:i + 4])[0]
        if 0xE1 <= m <= 0xEF or m == 0xFE:
            found.append(JPEG_NAMES.get(m, f'APP{m - 0xE0}'))
        i += 2 + n
    return found


def png_meta(b):
    found, i = [], 8
    while i + 8 <= len(b):
        n, kind = struct.unpack('>I4s', b[i:i + 8])
        if kind in PNG_META:
            found.append(kind.decode())
        if kind == b'IEND':
            break
        i += 12 + n
    return found


def check(path):
    b = open(path, 'rb').read()
    if b[:2] == b'\xff\xd8':
        return jpeg_meta(b)
    if b[:8] == PNG_SIG:
        return png_meta(b)
    return [f'unrecognised format: {path.rsplit(".", 1)[-1]}']


def main(repos):
    n = bad = 0
    for r in repos:
        files = subprocess.run(['git', '-C', r, 'ls-files'], capture_output=True, text=True).stdout.split()
        for f in files:
            if not f.lower().endswith(('.jpg', '.jpeg', '.png', '.gif', '.webp', '.tif', '.tiff', '.heic')):
                continue
            n += 1
            found = check(os.path.join(r, f))
            if found:
                bad += 1
                print(f'  META  {os.path.join(r, f)}: {", ".join(sorted(set(found)))}')
    print(f'  {n} images, {bad} with metadata')
    return 1 if bad or not n else 0


if __name__ == '__main__':
    if len(sys.argv) < 2:
        sys.exit('usage: image-meta-check.py REPO [REPO ...]')
    sys.exit(main(sys.argv[1:]))
