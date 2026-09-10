#!/bin/bash
# Every gate in every repository, one output, one tally.
#
# Written 2026-09-09 because the fault that kept recurring was not a bad check
# but an unread one: a page regenerated and its audit then run in a DIFFERENT
# repository; a tally printed and pushed over, twice. Nine gates run by hand are
# nine chances to read only eight.
#
# It ends with "GATES FAILING: n", and that line is the whole point of it.
# About five minutes, nearly all of it regress.py.
#
#     bash all-gates.sh
#
# The doc-audit loop pairs README.md with index.html where there is no
# index.md, because three repositories build their page that way. Pairing on
# name alone skipped lasermade-tools, Gernreich.github.io and trumpet entirely
# and printed nothing for them, which is the failure this script exists to stop.
#
# NOT included, deliberately: flat-part-check over knotwork-soundholes,
# living-hinge and the bore sheets. It reports failures there that are its model
# and not the files -- a rosette is a cut-out with no single outline, a hinge
# slit is an open cut, a bore sheet carries twenty parts -- and each repository
# records which of its checks apply. Folding that noise in would make this tally
# unreadable, which is the one thing it must not be.
G=~/LaserMadeMusic/GIT/lasermade-tools
R=~/LaserMadeMusic/GIT
fail=0
say () { printf '%-46s %s\n' "$1" "$2"; [[ "$2" == *FAIL* ]] && fail=$((fail+1)); }

cd $R/trumpet/parts/bore/concept/swept-curve
for s in coupon serpentine opposed wave spiral dspiral volute; do
  for mode in "" "--port"; do
    o=$(python3 ribbon_bore.py --shape=$s $mode --no-write 2>&1)
    n=$(echo "$o" | grep -c '^  pass'); f=$(echo "$o" | grep -cE '^  FAIL|^error')
    say "ribbon $s ${mode:-plain}" "$( [ "$f" = 0 ] && [ "$n" = 12 ] && echo "ok  $n/12" || echo "FAIL $n pass $f fail")"
  done
done

cd $R/trumpet/tools
o=$(~/Software/boxes/venv/bin/python regress.py 2>&1 | tail -1)
say "regress.py, 26 block designs" "$( [[ "$o" == *"all designs pass"* ]] && echo "ok" || echo "FAIL $o")"

# regress.py MEASURES the committed sheets; it never redraws them. Every
# invariant can hold while the SVG on disk is one the current code would no
# longer produce -- the hole this file closed for the 40 ribbon sheets and left
# open on the 84 Boxes.py draws. repro.py redraws them. Its second direction is
# the one that matters: the 18 sheets describing the coil that was actually cut
# are checked against pinned hashes instead, so a regenerate sweep that
# overwrites the record of the instrument fails here rather than passing every
# invariant in silence.
o=$(~/Software/boxes/venv/bin/python repro.py 2>&1 | tail -1)
say "block sheets reproduce, as-built pinned" "$( [[ "$o" == *"failing"* ]] && echo "FAIL ${o# }" || echo "ok  ${o# }")"

for repo in bullroarer buzz-disc kalimba knotwork-soundholes living-hinge slapstick lasermade-tools Gernreich.github.io trumpet; do
  cd $R/$repo 2>/dev/null || continue
  bad=0; n=0
  for md in *.md; do
    h="${md%.md}.html"
    # README.md builds index.html in some repositories and README.html in none.
    # Pairing on name alone skipped lasermade-tools, Gernreich.github.io and
    # trumpet entirely, and printed nothing at all for them.
    [ -f "$h" ] || { [ "$md" = README.md ] && [ -f index.html ] && [ ! -f index.md ] && h=index.html || continue; }
    n=$((n+1))
    echo "$(python3 $G/doc-audit.py "$md" --html "$h" --rebuild "python3 $G/md2html.py {md} {out}" 2>&1 | grep -E 'passed,')" | grep -q ' 0 failed' || bad=$((bad+1))
  done
  [ $n -gt 0 ] && say "doc-audit $repo ($n pages)" "$( [ $bad = 0 ] && echo ok || echo "FAIL $bad page(s)")"
done

o=$(python3 $G/svg-stroke-check.py --dir $R --quiet 2>&1 | tail -1)
say "svg-stroke-check, every repo" "$( [[ "$o" == *"0 conflicting"* ]] && echo "ok  ${o% file*} files" || echo "FAIL $o")"

# The doc-audit gate above pairs a *.md with its published page, so it only ever
# saw the 9 documents at the repository roots. The other 18 -- every CLAUDE.md
# in the trumpet tree, the design notes that say what may and may not be recut --
# went unaudited from the day they were written. They held 20 failures. Most
# were the checker being wrong about a genre it had never been pointed at
# (indented output read as prose, "393 checks" read as a list claim), but one
# was real: a "Two things to know" over three bullets. A document nothing checks
# is a document that drifts, and these are the ones a person reads before
# cutting.
cd $R
u=0; n=0
for f in $(find bullroarer buzz-disc kalimba knotwork-soundholes living-hinge \
                slapstick lasermade-tools Gernreich.github.io trumpet \
                -name '*.md' -not -path '*/.git/*' | sort); do
  n=$((n+1))
  (cd "$(dirname "$f")" && python3 $G/doc-audit.py "$(basename "$f")" 2>&1 \
     | grep -q '0 failed') || u=$((u+1))
done
say "doc-audit, every markdown in every repo" "$( [ $u = 0 ] && echo "ok  $n docs" || echo "FAIL $u of $n")"

o=$(python3 $G/flat-part-check.py --dir $R/bullroarer --dir $R/buzz-disc 2>&1 | tail -1)
say "flat-part-check, flat parts" "$( [[ "$o" == *"0 failed"* ]] && echo "ok  $o" || echo "FAIL $o")"

# --- what the committed artefacts claim about themselves ---------------------
# The four below were verified by hand all through 2026-09-09 and by nothing
# else. A gate that runs the generators but never compares them with what ships
# cannot tell you the sheets on disk are the sheets the code draws, which is
# this project's whole claim.

cd $R/trumpet/parts/bore/concept/swept-curve
T=$(mktemp -d); same=0; bad=0
rr () { d=$1; stem=$2; shift 2
  for mode in "" "--port"; do
    suf=""; [ -n "$mode" ] && suf="-ported"
    python3 ribbon_bore.py "$@" $mode --out=$T/$stem$suf.svg >/dev/null 2>&1
    for part in cheek-x2 panels; do
      cmp -s "$d/cut-files/$stem$suf-$part-cut-files.svg" \
             "$T/$stem$suf-$part-cut-files.svg" && same=$((same+1)) || bad=$((bad+1))
    done
  done; }
rr coupon/ribbon-coupon-bore10-30deg-R30 ribbon-coupon-bore10-30deg-R30-180turn --shape=coupon
rr serpentine/ribbon-serpentine-bore10-30deg-3lobes-R72 ribbon-serpentine-bore10-30deg-3lobes-R72-1000mm --shape=serpentine
rr opposed/ribbon-opposed-bore10-30deg-3lobes-R64 ribbon-opposed-bore10-30deg-3lobes-R64-1000mm --shape=opposed
rr wave/ribbon-wave-bore10-45deg-5arc ribbon-wave-bore10-45deg-5arc-836mm --shape=wave
rr spiral/ribbon-spiral-bore10-45deg-R35to113 ribbon-spiral-bore10-45deg-R35to113-1000mm --shape=spiral
rr spiral/ribbon-spiral-bore10-45deg-R36to144 ribbon-spiral-bore10-45deg-R36to144-1767mm --shape=spiral --spiral-facets=25 --spiral-ri=36.5 --spiral-ro=144
rr spiral/ribbon-spiral-bore10-45deg-R74to144 ribbon-spiral-bore10-45deg-R74to144-1458mm --shape=spiral --spiral-facets=17 --spiral-ri=74 --spiral-ro=144
rr dspiral/ribbon-dspiral-bore10-30deg-R62-pitch46 ribbon-dspiral-bore10-30deg-R62-pitch46-1506mm --shape=dspiral
rr dspiral/ribbon-dspiral-bore10-30deg-R62-pitch46-halftest ribbon-dspiral-bore10-30deg-R62-pitch46-half-196mm --shape=dspiral --ds-half --ds-facets=2
rr volute/ribbon-volute-bore10-45deg-R94-step60 ribbon-volute-bore10-45deg-R94-step60-1180mm --shape=volute
rm -rf $T
say "ribbon sheets reproduce byte-identical" "$( [ $bad = 0 ] && echo "ok  $same/40" || echo "FAIL $bad differ")"

stale=0
for f in */*/cut-files/*.svg; do
  python3 $G/make-preview.py "$f" /tmp/_ag.svg >/dev/null 2>&1
  cmp -s /tmp/_ag.svg "previews/$(basename "$f")" || stale=$((stale+1))
done; rm -f /tmp/_ag.svg
say "previews current with their cut files" "$( [ $stale = 0 ] && echo "ok  40/40" || echo "FAIL $stale stale")"

cd $R/trumpet/parts/bore/concept/walk/no-elbows/coil/search
cp parts.json /tmp/_ag_pj; cp SCORING.md /tmp/_ag_sc
node tools/parts.js >/dev/null 2>&1; node tools/gen_scoring.js >/dev/null 2>&1
ok1=$(cmp -s /tmp/_ag_pj parts.json && echo y); ok2=$(cmp -s /tmp/_ag_sc SCORING.md && echo y)
rm -f /tmp/_ag_pj /tmp/_ag_sc
say "search tools reproduce their output" "$( [ "$ok1$ok2" = yy ] && echo ok || echo "FAIL")"

cd $R/trumpet/tools
o=$(python3 -c "
import ast,pathlib,os,sys
bad=[f for f in ('snakebox.py','snakeboxvar.py')
     if ast.dump(ast.parse(pathlib.Path(f).read_text()))
     != ast.dump(ast.parse(pathlib.Path(os.path.expanduser('~/Software/boxes/boxes/generators/'+f)).read_text()))]
print(','.join(bad) if bad else 'ok')" 2>&1)
say "Boxes install matches tools/" "$( [ "$o" = ok ] && echo ok || echo "FAIL $o")"

cd $R
d=0; for r in */; do d=$((d + $(git -C $r status --porcelain 2>/dev/null | wc -l))); done
say "every repo clean" "$( [ $d = 0 ] && echo ok || echo "FAIL $d changed")"
u=0; for r in */; do u=$((u + $(git -C $r rev-list --count @{u}..HEAD 2>/dev/null || echo 0))); done
say "every repo pushed" "$( [ $u = 0 ] && echo ok || echo "FAIL $u unpushed")"

echo
echo "GATES FAILING: $fail"

