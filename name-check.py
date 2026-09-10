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
    rows = []
    for ln in open(path or GATES):
        m = re.match(r'rr (\S+/\S+) (\S+) (--\S.*)', ln.strip())
        if m:
            rows.append((m[2], m[3].split()))
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
                r'ribbon bore, (\S+)\s+(\d+)mm square, (\d+) degree facets', out)
            cl = re.search(r'centreline ([\d.]+)mm', out)
            if not head or not cl:
                print(f'  FAIL  {stem}: the generator printed no report')
                bad += 1
                continue
            shape, bore, deg, mm = head[1], int(head[2]), int(head[3]), float(cl[1])
            desc = out.split('\n')[1]
            claims = [('bore', int(re.search(r'bore(\d+)', stem)[1]), bore),
                      ('facet angle', int(re.search(r'-(\d+)deg', stem)[1]), deg)]
            if (m := re.search(r'-(\d+)mm$', stem)):
                claims.append(('centreline', int(m[1]), round(mm)))
            # radii: a span (R35to113) or a single start radius (R30, R62, R94)
            if (m := re.search(r'R(\d+)to(\d+)', stem)):
                got = [round(float(x)) for x in re.findall(r'R([\d.]+)', desc)]
                claims += [('inner R', int(m[1]), got[0] if got else None),
                           ('outer R', int(m[2]), got[1] if len(got) > 1 else None)]
            elif (m := re.search(r'-R(\d+)', stem)):
                got = re.search(r'R([\d.]+)', desc)
                claims.append(('radius', int(m[1]),
                               round(float(got[1])) if got else None))
            if (m := re.search(r'pitch(\d+)', stem)):
                g = re.search(r'rising ([\d.]+)mm a turn', desc)
                claims.append(('pitch', int(m[1]), round(float(g[1])) if g else None))
            if (m := re.search(r'step(\d+)', stem)):
                g = re.search(r'stepping ([\d.]+)mm a turn', desc)
                claims.append(('step', int(m[1]), round(float(g[1])) if g else None))
            if (m := re.search(r'-(\d+)lobes', stem)):
                g = re.search(r'(\d+) half-circles', desc)
                claims.append(('lobes', int(m[1]), int(g[1]) if g else None))
            claims.append(('shape', stem.split('-')[1], shape))
            for what, said, got in claims:
                if said != got:
                    print(f'  FAIL  {stem}: name says {what} {said}, '
                          f'the generator says {got}')
                    bad += 1
            print(f'  ok    {stem[:56]:56} {len(claims)} claims')
    print(f'\n  {len(rows)} designs, {bad} name(s) disagreeing'
          if bad else f'\n  {len(rows)} designs, every name agrees')
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else None))
