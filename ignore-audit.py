#!/usr/bin/env python3
"""Does every .doc-audit-ignore entry still suppress something?

Each line of an ignore file is a standing claim -- "this name is prose, not a
file here" -- and every gate in this repository checks artefacts, code or
documents. Nothing checks the SUPPRESSIONS. An exemption that has stopped doing
anything is worse than no exemption at all: it is an assertion nobody re-reads,
sitting in the one list whose entire job is to be trusted, and the longer such a
list runs on the less anyone questions any line of it.

Method: for each entry, run every document in the repository against a COPY of
the ignore file with that one line removed, and see whether anything starts
failing. If nothing does, the entry is dead weight. The copy matters -- editing
the repository's own file in a loop is how check 24 left five design stems
corrupted when it was interrupted.

Found two on 2026-09-11: trumpet exempted parts/LICENSE and tools/LICENSE from
the orphan check, which never needed it, because that check matches a tracked
file by BASENAME as well as by path and the README's own mention of LICENSE
already covered every copy.
"""
import os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
AUDIT = os.path.join(HERE, 'doc-audit.py')
ROOT = os.path.expanduser('~/LaserMadeMusic/GIT')


# Both suppression files, not just one. .doc-audit-ignore exempts a NAME and
# .doc-audit-generated exempts a DIRECTORY -- six of those lines exempt 378
# tracked files in trumpet, so it is much the blunter instrument and it went
# unaudited when the named exemptions were audited on 2026-09-11. Fixing one
# instance of a fault and not asking where else it lives is the fault this
# repository keeps rediscovering.
FILES = ['.doc-audit-ignore', '.doc-audit-generated']
FLAG = {'.doc-audit-ignore': '--ignore-file',
        '.doc-audit-generated': '--generated-file'}


def failures(repo, which, path):
    docs = subprocess.run(['git', '-C', repo, 'ls-files', '*.md'],
                          capture_output=True, text=True).stdout.split()
    n = 0
    for d in docs:
        r = subprocess.run([sys.executable, AUDIT, os.path.basename(d),
                            FLAG[which], path],
                           cwd=os.path.join(repo, os.path.dirname(d)),
                           capture_output=True, text=True)
        n += r.stdout.count('✗')
    return n


def main():
    dead = total = 0
    with tempfile.TemporaryDirectory() as T:
        for name in sorted(os.listdir(ROOT)):
            repo = os.path.join(ROOT, name)
            for which in FILES:
                f = os.path.join(repo, which)
                if not os.path.isfile(f):
                    continue
                lines = open(f).read().split('\n')
                entries = [l.strip() for l in lines
                           if l.strip() and not l.strip().startswith('#')]
                total += len(entries)
                # With the full list a repository must be clean, or "removing
                # this line changes nothing" cannot mean anything.
                if failures(repo, which, f):
                    print(f'  SKIP  {name} {which}: fails with the full list')
                    continue
                for e in entries:
                    tmp = os.path.join(T, 'list')
                    open(tmp, 'w').write(
                        '\n'.join(l for l in lines if l.strip() != e))
                    if failures(repo, which, tmp) == 0:
                        print(f'  DEAD  {name} {which}: {e} suppresses nothing')
                        dead += 1
    print(f'  {total} exemptions, {dead} suppressing nothing')
    return 1 if dead else 0


if __name__ == '__main__':
    sys.exit(main())
