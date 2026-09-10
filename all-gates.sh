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

o=$(python3 $G/flat-part-check.py --dir $R/bullroarer --dir $R/buzz-disc 2>&1 | tail -1)
say "flat-part-check, flat parts" "$( [[ "$o" == *"0 failed"* ]] && echo "ok  $o" || echo "FAIL $o")"

cd $R
d=0; for r in */; do d=$((d + $(git -C $r status --porcelain 2>/dev/null | wc -l))); done
say "every repo clean" "$( [ $d = 0 ] && echo ok || echo "FAIL $d changed")"
u=0; for r in */; do u=$((u + $(git -C $r rev-list --count @{u}..HEAD 2>/dev/null || echo 0))); done
say "every repo pushed" "$( [ $u = 0 ] && echo ok || echo "FAIL $u unpushed")"

echo
echo "GATES FAILING: $fail"

