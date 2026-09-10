#!/usr/bin/env python3
"""Do a repository's shipped drawings and pages still come out of its generators?

trumpet had this twice over -- byte gates for the ribbon sheets and the block
sheets -- and no other repository had it at all. knotwork-soundholes ships 13
cut files from two generators and living-hinge 14 from one, none of them
checked, and for four of the knots the command that drew them was recorded
NOWHERE: not in the README, not in the filename, not in a comment. They were
recovered on 2026-09-10 by reading the parameters back out of each SVG's own
description -- the ribbon width is 2*HW, the cosine amplitude is AMP, the rim
overrun is BITE -- and every one then reproduced byte-identical. This file
exists so that recovery never has to happen again.

The manifest is .repro in the repository root:

    shipped/path.svg :: shell command writing the file to $OUT

$OUT is a temp path; the command must put exactly that file there. Lines
starting with # are comments, blank lines ignored.
"""
import filecmp, os, shlex, subprocess, sys, tempfile

def main(repo, quiet=False):
    man = os.path.join(repo, '.repro')
    if not os.path.exists(man):
        print(f'  no .repro in {repo}')
        return 2
    rows = []
    for n, line in enumerate(open(man), 1):
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        if '::' not in line:
            print(f'  {man}:{n}: no :: separator')
            return 2
        name, _, cmd = line.partition('::')
        rows.append((name.strip(), cmd.strip()))
    # A command of "!" means: this file ships without a generator, we know, and
    # the reason is in the comment above it. It still has to be LISTED, so a new
    # unreproducible file is a decision someone writes down rather than a
    # silence. Display-only renderings are the honest case; a cut file should
    # never be one.
    acked = {n for n, c in rows if c == '!'}
    rows = [(n, c) for n, c in rows if c != '!']
    # A manifest that parses nothing passes. Say what was read, and treat a
    # shipped drawing or page that no line claims as a failure rather than a
    # silence.
    #
    # PAGES TOO, since 2026-09-10. This walked *.svg only, so every published
    # HTML page in reach of a manifest was unclaimed by construction: the two
    # part turn-pages, and the four built from markdown by md2html.py. The
    # markdown ones are compared a second way by doc-audit's page-currency
    # check, which is no argument for this one not seeing them -- doc-audit
    # pairs a page with a document of the same NAME, so a page whose source is
    # named differently, or which has no markdown at all, falls outside it
    # entirely. That is exactly where the turn pages sit.
    shipped = set()
    for base, _, files in os.walk(repo):
        if '.git' in base.split(os.sep):
            continue
        for f in files:
            if f.endswith(('.svg', '.html')):
                shipped.add(os.path.relpath(os.path.join(base, f), repo))
    same = bad = 0
    with tempfile.TemporaryDirectory() as T:
        for name, cmd in rows:
            target = os.path.join(repo, name)
            if not os.path.exists(target):
                print(f'  MISSING   {name}')
                bad += 1
                continue
            out = os.path.join(T, os.path.basename(name))
            env = dict(os.environ, OUT=out)
            r = subprocess.run(cmd, shell=True, cwd=repo, env=env,
                               capture_output=True, text=True)
            if not os.path.exists(out):
                last = ((r.stdout + r.stderr).strip().splitlines() or ['no output'])[-1]
                print(f'  NOT DRAWN {name}: {last[:80]}')
                bad += 1
            elif filecmp.cmp(target, out, shallow=False):
                same += 1
            else:
                print(f'  DIFFERS   {name}')
                bad += 1
    unclaimed = sorted(shipped - {n for n, _ in rows} - acked)
    for u in unclaimed:
        print(f'  UNCLAIMED {u}')
    # AND AN ACKNOWLEDGEMENT FOR A FILE THAT HAS GONE. A "!" line is a standing
    # claim that a particular file ships without a generator. Nothing checked
    # that the file was still there, so a deleted preview left its exemption
    # behind: an assertion nobody re-reads, in a list whose whole job is to be
    # trusted. Same fault ignore-audit.py exists to catch in the doc-audit
    # exemption lists, in the one list it does not read.
    stale = sorted(a for a in acked if not os.path.exists(os.path.join(repo, a)))
    for a in stale:
        print(f'  STALE !   {a}: acknowledged, and not there any more')
    tail = f', {len(acked)} shipped without a generator' if acked else ''
    lost = f', {len(stale)} acknowledged and gone' if stale else ''
    print(f'  {same} reproduce, {bad} failing, {len(unclaimed)} unclaimed'
          f'{tail}{lost}'
          if (bad or unclaimed or stale) else f'  {same} reproduce{tail}')
    return 1 if (bad or unclaimed or stale) else 0


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if not a.startswith('-')]
    sys.exit(main(args[0] if args else '.', '--quiet' in sys.argv))
