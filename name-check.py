#!/usr/bin/env python3
"""Does a cut file's NAME agree with the geometry inside it?

Every other gate compares an artefact with the code that draws it. None of them
reads the name. That is a gap with a shape: the reproduction gate passes the
stem in on the command line, so a design called -1180mm reproduces perfectly
whatever its centreline actually is, and a folder called R62-pitch46 keeps that
name after the pitch is changed underneath it.

The names are not decoration. They are what a person reads off a sheet at the
machine to know which bore they are holding, and the only place several of these
numbers appear at all.

Designs are read from all-gates.sh's own rr lines rather than listed again here,
so the two cannot drift apart. Claims checked, where the name makes them:

    bore10        the section, against "10mm square"
    30deg         the facet angle, against "30 degree facets"
    1180mm        the centreline, rounded, against "centreline 1179.9mm"
    R30 / R62     the radius the shape starts from
    R35to113      a spiral's inner and outer radius
    pitch46       an Archimedean spiral's rise per turn
    step60        a volute's step per turn
    3lobes        half-circles in a serpentine
    <shape>       the second word of the name, against what the generator says
"""
import os, re, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATES = os.path.join(HERE, 'all-gates.sh')
SWEPT = os.path.expanduser(
    '~/LaserMadeMusic/GIT/trumpet/parts/bore/concept/swept-curve')


def designs(path=None):
    """Read the design table. Takes a path so this check can be tested against
    a COPY of all-gates.sh with a name deliberately falsified, instead of
    editing the live harness in place -- which is how the first attempt at
    proving it can fail left five stems corrupted mid-loop."""
    rows, seen = [], set()
    for ln in open(path or GATES):
        # The rr line grew a SUFFIX field on 2026-09-13, when the shipped
        # sheets became --narrow and the ported ones split into round-port and
        # square-port variants. The old three-field pattern matched none of the
        # new lines, and this check reported "no rr lines found" rather than a
        # wrong name -- which is the refusal the empty-list guard below exists
        # for, working exactly as intended.
        # 'rrc' as well as 'rr': --port-per-cheek writes two cheek sheets
        # instead of one, so it has its own helper in all-gates.sh -- and a
        # pattern anchored to 'rr ' alone silently dropped those designs out of
        # this table, which is the failure this file's own guard is about.
        m = re.match(r'rrc? (\S+/\S+) (\S+) (\S+) (--\S.*)', ln.strip())
        if m and m[2] not in seen:
            # One row per STEM. A stem now appears two or three times, once per
            # port variant, and every claim this file checks -- bore, facet,
            # radius, pitch, step, lobes, shape -- is a property of the stem and
            # identical across them. Checking it three times would say "30
            # designs" over ten, and a count that overstates what was examined
            # is the one thing a gate must not do.
            seen.add(m[2])
            rows.append((m[2], m[4].split()))
    return rows


def main(path=None):
    rows = designs(path)
    # A check that parses nothing passes. Say what was read, and refuse to
    # report agreement on an empty list.
    if not rows:
        print('  FAIL  no rr lines found in all-gates.sh')
        return 1
    bad = 0
    with tempfile.TemporaryDirectory() as T:
        for stem, sw in rows:
            r = subprocess.run([sys.executable, 'ribbon_bore.py', *sw,
                                f'--out={T}/x.svg'],
                               cwd=SWEPT, capture_output=True, text=True)
            out = r.stdout
            head = re.search(
                r'ribbon bore, (\S+)\s+(\d+)mm square, ([\d.]+) degree facets',
                out)
            cl = re.search(r'centreline ([\d.]+)mm', out)
            if not head or not cl:
                print(f'  FAIL  {stem}: the generator printed no report')
                bad += 1
                continue
            shape, bore, deg, mm = (head[1], int(head[2]), float(head[3]),
                                    float(cl[1]))
            # The shape line is the one AFTER the header, not stdout line 1.
            # --merge-lead prints before the report, so for a design whose only
            # rr row is a merged one -- the 1000mm double spiral is the first --
            # line 1 was the header itself, and every radius and pitch claim
            # read None and "disagreed" with a name that was perfectly right.
            # Every other design's first row is its unported variant, which
            # prints nothing ahead of the report, so this sat here unseen.
            lines = out.split('\n')
            desc = lines[lines.index(head[0]) + 1] if head[0] in lines else ''
            claims = [('bore', int(re.search(r'bore(\d+)', stem)[1]), bore),
                      ('facet angle',
                       re.search(r'-([\d.]+)deg', stem)[1], deg)]
            if (m := re.search(r'-(\d+)mm$', stem)):
                claims.append(('centreline', int(m[1]), round(mm)))
            # radii: a span (R35to113) or a single start radius (R30, R62, R94)
            if (m := re.search(r'R(\d+)to(\d+)', stem)):
                got = [round(float(x)) for x in re.findall(r'R([\d.]+)', desc)]
                claims += [('inner R', int(m[1]), got[0] if got else None),
                           ('outer R', int(m[2]), got[1] if len(got) > 1 else None)]
            elif (m := re.search(r'-R([\d.]+)', stem)):
                got = re.search(r'R([\d.]+)', desc)
                claims.append(('radius', m[1],
                               float(got[1]) if got else None))
            if (m := re.search(r'pitch(\d+)', stem)):
                g = re.search(r'rising ([\d.]+)mm a turn', desc)
                claims.append(('pitch', int(m[1]), round(float(g[1])) if g else None))
            if (m := re.search(r'step(\d+)', stem)):
                g = re.search(r'stepping ([\d.]+)mm a turn', desc)
                claims.append(('step', int(m[1]), round(float(g[1])) if g else None))
            if (m := re.search(r'-(\d+)lobes', stem)):
                # "half-circles" is the OPEN serpentine's word and only its
                # word. A closed serpentine's lobes are a 108 degree bulge
                # against a 36 degree scoop -- lobes, and not half of anything
                # -- so its description says "5 lobes" and this read None and
                # called a correct name wrong. Either spelling counts.
                g = re.search(r'(\d+) (?:half-circles|lobes)', desc)
                claims.append(('lobes', int(m[1]), int(g[1]) if g else None))
            claims.append(('shape', stem.split('-')[1], shape))
            wrong = 0
            for what, said, got in claims:
                # A NAME IS HELD TO THE PRECISION IT STATES, and no further.
                # Two conventions have to live together here. -R72 names a
                # serpentine whose radius is really 71.754 -- solved to give
                # exactly 1000mm, and rounded in the name on purpose -- so an
                # exact compare calls a correct name wrong. -R128.572 and
                # -27.6923deg name a ring that cannot be written any shorter:
                # a ring's facet angle is 360/n, a whole number only when n
                # divides 360, and rounding 27.6923 to 27 throws away the only
                # digits that tell one ring from the next. Both are right, and
                # what separates them is how many decimals the NAME spends.
                # So round the generator's figure to the name's own precision
                # and compare there: 71.754 to 0dp is 72, 128.571738 to 3dp is
                # 128.572, and neither convention has to give way.
                if isinstance(said, str) and re.fullmatch(r'[\d.]+', said):
                    dp = len(said.split('.')[1]) if '.' in said else 0
                    ok = got is not None and round(float(got), dp) == float(said)
                else:
                    ok = said == got
                if not ok:
                    print(f'  FAIL  {stem}: name says {what} {said}, '
                          f'the generator says {got}')
                    bad += 1
                    wrong += 1
            # The ok line was printed unconditionally, so a design that had just
            # reported failures got an "ok" of its own underneath them. The
            # tally at the end was right; the line a reader stops at was not.
            print(f'  ok    {stem[:56]:56} {len(claims)} claims' if not wrong
                  else f'  ----  {stem[:56]:56} {wrong} of {len(claims)} wrong')
    print(f'\n  {len(rows)} designs, {bad} name(s) disagreeing'
          if bad else f'\n  {len(rows)} designs, every name agrees')
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else None))
