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
# EVERY verdict below is fail-CLOSED, and was not until 2026-09-11.
#
# The idiom this file grew was "FAIL if the output contains a failure word,
# otherwise ok" -- which passes on an empty string, so a tool that crashed and
# printed nothing read as a pass. Three gates were built that way. Four more
# counted their subjects and compared the count of failures to zero, so an empty
# subject list gave "ok 0/0": the glob matching nothing, which is precisely the
# failure this script's own header says it exists to stop. And a repository with
# no pages at all was skipped without printing a line.
#
# So a gate now has to see the SUCCESS it is looking for, and has to have found
# something to look at. Absence of bad news is not good news.
fail=0
failed=()
say () {
  printf '%-46s %s\n' "$1" "$2"
  if [[ "$2" == *FAIL* ]]; then fail=$((fail+1)); failed+=("$1  --  $2"); fi
}

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
# Exit status AND the success line. Check 27 made these verdicts require the
# words they are looking for, which closes the "printed nothing" hole but not
# the "printed the success line and then crashed" one. These four tools were
# tested and do exit non-zero on failure, so use both.
ro=$(~/Software/boxes/venv/bin/python repro.py 2>&1); rc=$?
o=$(echo "$ro" | tail -1); [ $rc = 0 ] || o="tool exited $rc"
say "block sheets reproduce, as-built pinned" "$( [[ "$o" == *reproduce* && "$o" != *failing* ]] && echo "ok  ${o# }" || echo "FAIL ${o:-no output}")"

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
  say "doc-audit $repo ($n pages)" "$( [ $bad = 0 ] && [ $n -gt 0 ] && echo ok || echo "FAIL ${bad} page(s), $n found")"
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
say "doc-audit, every markdown in every repo" "$( [ $u = 0 ] && [ $n -gt 20 ] && echo "ok  $n docs" || echo "FAIL $u of $n")"

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
say "ribbon sheets reproduce byte-identical" "$( [ $bad = 0 ] && [ $same = 40 ] && echo "ok  $same/40" || echo "FAIL $bad differ, $same of 40 compared")"

# Walks EVERY previews/ directory in the repository, not just this one. It
# checked swept-curve/previews and nothing else, so parts/bell/previews,
# tools/coupon-16mm/previews and a stray single preview under the frozen 1.5t
# coil -- three files -- were watched by nothing. All three were current; that
# they were is luck rather than a result, since nothing would have said
# otherwise. Counted rather than hardcoded at 40, because a number written into
# a gate stops being a measurement the moment a file is added.
cd $R/trumpet
stale=0; seen=0
for pv in $(find . -type d -name previews -not -path './.git/*'); do
  d=$(dirname "$pv")
  for f in "$pv"/*.svg; do
    [ -e "$f" ] || continue
    b=$(basename "$f")
    src=""
    for cand in "$d/$b" "$d/cut-files/$b" "$d"/*/cut-files/"$b" \
                "$d"/*/*/cut-files/"$b"; do
      [ -f "$cand" ] && { src="$cand"; break; }
    done
    seen=$((seen+1))
    [ -n "$src" ] || { stale=$((stale+1)); echo "  preview with no cut file: $b"; continue; }
    # Remove the target FIRST. This is a fixed path reused round a loop, so a
    # generator that failed on one file left the previous file's preview lying
    # there to be compared against -- the wrong two things, silently.
    rm -f /tmp/_ag.svg
    python3 $G/make-preview.py "$src" /tmp/_ag.svg >/dev/null 2>&1
    if [ ! -f /tmp/_ag.svg ]; then
      stale=$((stale+1)); echo "  preview would not draw: $b"
    else
      cmp -s /tmp/_ag.svg "$f" || stale=$((stale+1))
    fi
  done
done; rm -f /tmp/_ag.svg
say "previews current with their cut files" "$( [ $stale = 0 ] && [ $seen -gt 0 ] && echo "ok  $seen/$seen" || echo "FAIL $stale of $seen stale")"

cd $R/trumpet/parts/bore/concept/walk/no-elbows/coil/search
# The generators' EXIT STATUS is checked, not just the files afterwards. Until
# 2026-09-11 this ran them with output discarded and then compared each file
# with its own backup -- so a script that crashed left the file untouched and
# the comparison passed. The gate could not tell "reproduced correctly" from
# "did not run at all", which is the one distinction it exists to make.
# Remove the backups FIRST, for the reason the preview loop above gives: these are
# fixed paths, and a run interrupted before the rm at the end leaves them lying there.
# cp then fails silently on a file that has been DELETED from the repository, the
# generator recreates it, and cmp compares it against yesterday's copy and passes --
# so a deletion like a7e36bc, which is why README.md is watched here at all, would
# read as ok 3/3.
rm -f /tmp/_ag_pj /tmp/_ag_sc /tmp/_ag_rm
cp parts.json /tmp/_ag_pj; cp SCORING.md /tmp/_ag_sc; cp README.md /tmp/_ag_rm
node tools/parts.js >/dev/null 2>&1;      r1=$?
node tools/gen_scoring.js >/dev/null 2>&1; r2=$?
node tools/gen_readme.js >/dev/null 2>&1;  r3=$?
ok1=$([ $r1 = 0 ] && cmp -s /tmp/_ag_pj parts.json && echo y)
ok2=$([ $r2 = 0 ] && cmp -s /tmp/_ag_sc SCORING.md && echo y)
# README.md joined these on 2026-09-11. It is generated by gen_readme.js like the
# other two, and was the third generated artefact in this directory with nothing
# watching it -- restored after the repository-wide README removal deleted it and
# left its generator behind.
ok3=$([ $r3 = 0 ] && cmp -s /tmp/_ag_rm README.md && echo y)
rm -f /tmp/_ag_pj /tmp/_ag_sc /tmp/_ag_rm
say "search tools reproduce their output" "$( [ "$ok1$ok2$ok3" = yyy ] && echo "ok  3/3" \
  || echo "FAIL parts.json=${ok1:-n} SCORING.md=${ok2:-n} README.md=${ok3:-n}")"

# Both reproduction gates above are trumpet-only, and every other repository
# that ships generator-drawn SVGs had none. knotwork-soundholes draws 13 cut
# files and living-hinge 15, and nothing checked any of them. They all do
# reproduce -- but for four of the knots the command was recorded NOWHERE, and
# had to be recovered on 2026-09-10 by reading the parameters back out of each
# SVG's own description. .repro in each repository records them now.
for repo in knotwork-soundholes living-hinge; do
  cd $R/$repo 2>/dev/null || continue
  ro=$(python3 $G/repro-svg.py . 2>&1); rc=$?
  o=$(echo "$ro" | tail -1); [ $rc = 0 ] || o="tool exited $rc"
  say "$repo SVGs reproduce" "$( [[ "$o" == *reproduce* && "$o" != *failing* && "$o" != *unclaimed* ]] && echo "ok  ${o# }" || echo "FAIL ${o:-no output}")"
done

# The previews in the four hand-drawn repositories are made by make-preview.py
# and were current with it, unwatched, the whole time. trumpet's are gated and
# these were not, for no reason other than that nobody had looked.
cd $R
stale=0; n=0
for repo in bullroarer buzz-disc kalimba slapstick; do
  for f in $repo/previews/*.svg; do
    [ -e "$f" ] || continue
    src="$repo/$(basename "$f")"
    [ -f "$src" ] || continue
    n=$((n+1))
    # Same fixed-path-round-a-loop hazard the trumpet preview loop above guards
    # against, and this loop did not: without the rm, a generator that failed on
    # one source left the PREVIOUS source's preview lying there to be compared,
    # and the gate counted it ok.
    rm -f /tmp/_ag2.svg
    python3 $G/make-preview.py "$src" /tmp/_ag2.svg >/dev/null 2>&1
    if [ ! -f /tmp/_ag2.svg ]; then
      stale=$((stale+1)); echo "  preview would not draw: $(basename "$f")"
    else
      cmp -s /tmp/_ag2.svg "$f" || stale=$((stale+1))
    fi
  done
done
rm -f /tmp/_ag2.svg
say "previews current in the hand-drawn repos" "$( [ $stale = 0 ] && [ $n -gt 0 ] && echo "ok  $n/$n" || echo "FAIL $stale stale of $n")"

# The one artefact that CROSSES repositories: gernreich.github.io publishes an
# embed drawn by a generator that lives in trumpet, and its own notes said in
# so many words "Nothing gates it." It had drifted -- the published copy predated
# the field naming which curve each shape's vertices sit on, so the file on the
# site was not the file the generator draws. Inert, as it happens: the embed is
# canvas-only and never reads that field. The next one need not be.
cd $R/trumpet/parts/bore/concept/swept-curve
# Leftover and exit status both matter here, for the two reasons the gates above
# give: a run interrupted before the rm leaves the file behind, and the
# generator's status was thrown away, so a crash that wrote nothing was compared
# against that leftover and read as ok.
rm -f /tmp/_ag_bv.html
python3 ribbon_view.py --shape=serpentine --embed --out=/tmp/_ag_bv.html \
    --home=https://gernreich.github.io/trumpet/ >/dev/null 2>&1; rv=$?
if [ $rv != 0 ] || [ ! -f /tmp/_ag_bv.html ]; then o="FAIL generator exited $rv"
elif cmp -s /tmp/_ag_bv.html $R/Gernreich.github.io/bore-viewer.html; then o=ok
else o="FAIL stale"; fi
rm -f /tmp/_ag_bv.html
say "published embed matches its generator" "$o"

# Every gate above checks an ARTEFACT. A tool that ships no artefact is checked
# by nothing at all, and five of them were: coils.py, mcwalk.py, nest.py,
# piece_render.py and sizes.py in trumpet, test-ladder.py here. sizes.py had
# been raising AttributeError on every run since 4cbf437 renamed the function it
# calls -- five days, truncating its own output file each time, and nothing said
# so. This runs each entry point once and asks only that it exit 0. That is a
# low bar and it is the bar that was missing; the tools with shipped output are
# held to byte-identity elsewhere.
cd $R/trumpet/tools
PYB=~/Software/boxes/venv/bin/python
T=$(mktemp -d); bad=0; n=0
run() { n=$((n+1)); "$@" >/dev/null 2>&1 || { bad=$((bad+1)); echo "  FAILS: $2"; }; }
run $PYB nest.py "N N10 U2 W2 S7 U2 E4 N9 W2 D2 N4 N" --out $T/n.svg
run $PYB sizes.py coil_fold2 $T/s.html
run $PYB piece_render.py --out $T/p.svg
run $PYB coils.py
run $PYB $G/test-ladder.py $T/ladder.svg
# mcwalk.py searches walks and has no bounded run, so it is asked only to import
run $PYB -c "import sys; sys.path.insert(0,'.'); import mcwalk"
rm -rf $T
say "every entry-point tool still runs" "$( [ $bad = 0 ] && [ $n -ge 6 ] && echo "ok  $n/$n" || echo "FAIL $bad of $n")"

# Nothing reads the NAMES. Every gate above compares an artefact with the code
# that draws it, and the reproduction gate is handed the stem on the command
# line -- so a design called -1180mm reproduces perfectly whatever its
# centreline is, and a folder called R62-pitch46 keeps that name after the pitch
# moves underneath it. The names are what a person reads off a sheet at the
# machine, and for several of these numbers they are the only record.
cd $G
ro=$(python3 name-check.py 2>&1); rc=$?
o=$(echo "$ro" | tail -1); [ $rc = 0 ] || o="tool exited $rc"
say "cut-file names match their geometry" "$( [[ "$o" == *"every name agrees"* ]] && echo "ok  ${o# }" || echo "FAIL ${o:-no output}")"

# And nothing checked the SUPPRESSIONS. Every .doc-audit-ignore line is a
# standing claim that a name is prose rather than a file, and an exemption that
# has stopped suppressing anything is worse than none: an assertion nobody
# re-reads, in the one list whose whole job is to be trusted.
cd $G
ro=$(python3 ignore-audit.py 2>&1); rc=$?
o=$(echo "$ro" | tail -1); [ $rc = 0 ] || o="tool exited $rc"
say "every exemption still suppresses something" "$( [[ "$o" == *", 0 suppressing"* ]] && echo "ok  ${o# }" || echo "FAIL ${o# }")"

cd $R/trumpet/tools
o=$(python3 -c "
import ast,pathlib,os,sys
bad=[f for f in ('snakebox.py','snakeboxvar.py')
     if ast.dump(ast.parse(pathlib.Path(f).read_text()))
     != ast.dump(ast.parse(pathlib.Path(os.path.expanduser('~/Software/boxes/boxes/generators/'+f)).read_text()))]
print(','.join(bad) if bad else 'ok')" 2>&1)
say "Boxes install matches tools/" "$( [ "$o" = ok ] && echo ok || echo "FAIL $o")"

cd $R
# Counts the repositories as well as the changes: with no repositories at all
# both come out zero, and "every repo clean" over nothing is not a result.
d=0; nr=0
for r in */; do
  [ -d "$r/.git" ] || continue
  nr=$((nr+1)); d=$((d + $(git -C $r status --porcelain 2>/dev/null | wc -l)))
done
say "every repo clean" "$( [ $d = 0 ] && [ $nr -ge 9 ] && echo "ok  $nr repos" || echo "FAIL $d changed in $nr repos")"
# --pre-push skips this one gate, and exists because gating a push on a clean
# run could otherwise never push: "every repo pushed" fails precisely BECAUSE
# the push has not happened yet, so
#
#     bash all-gates.sh && git push
#
# was unsatisfiable the moment there was anything to push. Every other gate here
# reports a defect; this one reports a state, and it is the only gate whose
# failure the very next command is meant to fix. Without the flag the honest
# workflow is:
#
#     bash all-gates.sh --pre-push && git push origin main
#
if [[ " $* " != *" --pre-push "* ]]; then
  u=0; nr=0
  for r in */; do
    [ -d "$r/.git" ] || continue
    nr=$((nr+1)); u=$((u + $(git -C $r rev-list --count @{u}..HEAD 2>/dev/null || echo 0)))
  done
  say "every repo pushed" "$( [ $u = 0 ] && [ $nr -ge 9 ] && echo "ok  $nr repos" || echo "FAIL $u unpushed across $nr repos")"
fi

echo
# The failing gates are NAMED here, and the script EXITS non-zero.
#
# Until 2026-09-11 it ended on an echo, so it returned 0 however many gates
# failed: `all-gates.sh && git push` pushed regardless, and the only signal was
# a line of text. This file's own header records the fault it was written to
# stop -- "a tally printed and pushed over, twice" -- and its answer to that was
# another line to read. It was then pushed over four more times in one session.
#
# So: a caller can now branch on it, and the names sit at the END, where someone
# reading the last few lines cannot see clean-and-pushed without also seeing
# what failed.
if (( fail )); then
  echo "FAILING GATES:"
  for f in "${failed[@]}"; do echo "  $f"; done
  echo
fi
echo "GATES FAILING: $fail"
exit $(( fail > 0 ))

