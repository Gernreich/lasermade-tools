#!/bin/bash
# Every gate in every repository, one output, one tally.
#
# Written 2026-09-09 because the fault that kept recurring was not a bad check
# but an unread one: a page regenerated and its audit then run in a DIFFERENT
# repository; a tally printed and pushed over, twice. Nine gates run by hand are
# nine chances to read only eight.
#
# It ends with "GATES FAILING: n", and that line is the whole point of it.
# About eight minutes: regress.py is most of it, and run_checks.sh another two
# and a half.
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
say "regress.py, 27 block designs" "$( [[ "$o" == *"all designs pass"* ]] && echo "ok" || echo "FAIL $o")"

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
# index.html and SCORING.html joined these on 2026-09-12. They are published pages built
# from the two markdown files by md2html.py, and were watched by nothing: the doc-audit
# pairing gate above only walks *.md at a repository ROOT, so it has never seen this
# directory, and this gate watched the sources but not what is served. Verified by
# appending a line to index.html and rerunning: every gate here passed, and only the
# clean-tree gate noticed, and only because the change was uncommitted. Committed, it
# would have gone out stale and silent.
# So the whole of build.sh runs, not the three node tools, and all five artefacts are
# compared. README.md joined on 2026-09-11, having been the third generated file here
# with nothing watching it.
rm -f /tmp/_ag_pj /tmp/_ag_sc /tmp/_ag_rm /tmp/_ag_ix /tmp/_ag_sh
cp parts.json /tmp/_ag_pj; cp SCORING.md /tmp/_ag_sc;   cp README.md /tmp/_ag_rm
cp index.html /tmp/_ag_ix; cp SCORING.html /tmp/_ag_sh
# The build's exit status decides all five, for the reason the tools were run separately
# before: these files are written in place, so a build that dies halfway leaves the rest
# untouched and byte-identical to their own backups. Without the status that reads as ok.
MD2HTML=$G/md2html.py bash tools/build.sh >/dev/null 2>&1; rb=$?
ok1=$([ $rb = 0 ] && cmp -s /tmp/_ag_pj parts.json    && echo y)
ok2=$([ $rb = 0 ] && cmp -s /tmp/_ag_sc SCORING.md    && echo y)
ok3=$([ $rb = 0 ] && cmp -s /tmp/_ag_rm README.md     && echo y)
ok4=$([ $rb = 0 ] && cmp -s /tmp/_ag_ix index.html    && echo y)
ok5=$([ $rb = 0 ] && cmp -s /tmp/_ag_sh SCORING.html  && echo y)
rm -f /tmp/_ag_pj /tmp/_ag_sc /tmp/_ag_rm /tmp/_ag_ix /tmp/_ag_sh
say "search tools reproduce their output" "$( [ "$ok1$ok2$ok3$ok4$ok5" = yyyyy ] && echo "ok  5/5" \
  || echo "FAIL build=$rb parts.json=${ok1:-n} SCORING.md=${ok2:-n} README.md=${ok3:-n} index.html=${ok4:-n} SCORING.html=${ok5:-n}")"

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
  say "$repo drawings and pages reproduce" "$( [[ "$o" == *reproduce* && "$o" != *failing* && "$o" != *unclaimed* ]] && echo "ok  ${o# }" || echo "FAIL ${o:-no output}")"
done

# And trumpet's OTHER two part directories, which had no reproduction gate of
# any kind. The block bores had one, the ribbon sheets had one, and the bell and
# the mouthpiece -- seven shipped SVGs from six generators -- had none: the only
# thing that touched them was the previews gate, and that compares a preview
# with a cut file rather than a cut file with the code. Scoped to the two
# directories rather than run over trumpet whole, because the bore tree holds
# hundreds of sheets that the two byte gates above already account for.
for d in parts/bell parts/mouthpiece \
         parts/bore/concept/walk/no-elbows/coil/search; do
  cd $R/trumpet
  ro=$(python3 $G/repro-svg.py $d 2>&1); rc=$?
  o=$(echo "$ro" | tail -1); [ $rc = 0 ] || o="tool exited $rc"
  say "${d##*/} drawings and pages reproduce" "$( [[ "$o" == *reproduce* && "$o" != *failing* && "$o" != *unclaimed* ]] && echo "ok  ${o# }" || echo "FAIL ${o:-no output}")"
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
# THE PORTS PATH, which no design in regress.py can carry. A port lets a change
# of plane happen inside a piece and cuts the part count sharply -- the test
# bore goes from three pieces to two -- and it is kept because it works: with
# two sections assembled the joint closes, the bore carries on through it and
# the whole passage is sealed. What it costs is a joint with no tab, and the
# fingers of the cell whose plate was removed facing nothing. check.py is
# written to refuse it for exactly those two reasons plus the third that
# follows from them, so a ported design cannot sit in the corpus and be "pass".
#
# It was gated by nothing at all instead, and check.py had no --ports to be
# gated with. This pins the REFUSAL: three named failures and no fourth. A
# fourth means the ports path has broken in some new way; two means one of the
# objections has quietly stopped being raised.
cd $R/trumpet/tools
o=$(~/Software/boxes/venv/bin/python check.py "U U2 E2 S2 U2 U" --ports 2>&1)
tot=$(echo "$o" | tail -1); hit=0
for w in "the section closes round its bore" \
         "no wall finger left unengaged" \
         "seam has no port"; do
  echo "$o" | grep -q "^  $w *FAIL" && hit=$((hit+1))
done
say "the ports path refuses for its three reasons" "$( [[ "$tot" == *"3 failed"* ]] && [ $hit = 3 ] && echo "ok  ${tot# }" || echo "FAIL ${tot:-no output}, $hit of 3 expected")"

cd $R/trumpet/tools
PYB=~/Software/boxes/venv/bin/python
T=$(mktemp -d); bad=0; n=0
run() { n=$((n+1)); "$@" >/dev/null 2>&1 || { bad=$((bad+1)); echo "  FAILS: $2"; }; }
run $PYB nest.py "N N10 U2 W2 S7 U2 E4 N9 W2 D2 N4 N" --out $T/n.svg
run $PYB sizes.py coil_fold2 $T/s.html
run $PYB piece_render.py --out $T/p.svg
run $PYB coils.py
run $PYB $G/test-ladder.py $T/ladder.svg
# mcwalk.py was asked only to IMPORT, on the grounds that it "searches walks and
# has no bounded run". It does neither: there is no search anywhere in the file,
# and it takes a walk and writes one page, exactly like the tools around it.
# Importing proves the module parses and nothing else -- build() and main() went
# untested behind a claim about the tool that was not true of it.
run $PYB mcwalk.py "N N3 U3 W5 N10 E5 S8 W3 S3 N12 N" --out $T/w.html
# bore_render.py joins them on 2026-09-10. It could not be run without changing
# the tree -- a fixed output name in the current directory, no --out -- which is
# exactly why it had no line here. It has one now, so it does.
run $PYB bore_render.py "W D3 E4 N" --out=$T/r.svg
rm -rf $T
say "every entry-point tool still runs" "$( [ $bad = 0 ] && [ $n -ge 7 ] && echo "ok  $n/$n" || echo "FAIL $bad of $n")"

# THE BELL AND THE MOUTHPIECE. The comment above names six tools that were run
# by nothing and stops there; twelve more, every one under parts/, were in the
# same state. Seven of their shipped SVGs are now claimed by parts/bell/.repro
# and parts/mouthpiece/.repro below. These five write nothing that ships, so no
# manifest can reach them, and they are the ones that had never been run at all.
#
# number_rings.py REWRITES THE SHEET IT IS GIVEN, in place, so it is handed a
# copy in $T and never the tracked file. It is asked for --order=document
# because it refuses to guess the order on a sheet that could have been
# hand-nested, which is the right refusal and not a failure.
#
# ramp_bell.py was gated here for one day, as a tool expected to REFUSE. It has
# been deleted instead: it rewrote a stroke: style property and these sheets
# carry a stroke attribute on the group, so it matched nothing on every sheet in
# the repository and had no valid input left. See parts/CLAUDE.md.
cd $R/trumpet/parts
T=$(mktemp -d); bad=0; n=0
BS=bell/cut-files/bell-round10-153mm-17rings-x3-rim86-cut-files.svg
MS=mouthpiece/cut-files/mouthpiece-bore10-trumpet-parts-cut-files.svg
cp $BS $T/sheet.svg
run python3 bell/bell.py 20 --out=$T/b.svg
run python3 bell/verify_bell.py $BS
run python3 bell/number_rings.py $T/sheet.svg --order=document
run python3 mouthpiece/mouthpiece.py $T/mp.svg
run python3 mouthpiece/mouthpiece-cup.py $T/mc.svg
run python3 part-view.py $BS $T/t1.html
run python3 part-view.py $MS $T/t2.html
rm -rf $T
say "every bell and mouthpiece tool still runs" "$( [ $bad = 0 ] && [ $n -ge 7 ] && echo "ok  $n/$n" || echo "FAIL $bad of $n")"

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
# Asks BORE_SPLIT rather than re-implementing the comparison, and so looks at
# the checkout that actually draws. This had ~/Software/boxes written into it
# while bore_split.py resolves SNAKEBOX_BOXES first and falls back to a search,
# so with that variable set the gate and the writer were reading two different
# installs -- a gate that agrees with itself and not with the thing it watches.
# One copy of the rule now, in the file that depends on it.
o=$(python3 -c "
import sys
sys.path.insert(0, '.')
import bore_split as B
d = B._installed_matches_source()
print('; '.join(d) if d else 'ok')" 2>/dev/null)
# Empty means the import died before it could answer -- no checkout, a syntax
# error, anything. That is not agreement either.
[ -n "$o" ] || o="the drift check would not run at all"
say "Boxes install matches tools/" "$( [ "$o" = ok ] && echo ok || echo "FAIL $o")"

# THE CORPUS IS SUPPOSED TO BE IN STANDARD FORM -- north, counter-clockwise,
# whole periods nearest the common target, one block in and one out -- and that
# is the whole basis on which two coils in it are compared. Nothing checked it.
# standardise.js --write is idempotent on a standard corpus, so running it and
# requiring the tree to stay clean IS the check, the same way the transcripts
# below are checked by regenerating them. Three seconds.
#
# A walk that had drifted out of standard form would not be wrong, exactly: it
# would still cut. It would just no longer be comparable with the nine beside it,
# which is the one thing the whole search directory is for.
cd $R/trumpet/parts/bore/concept/walk/no-elbows/coil/search
o=$(node tools/standardise.js --write 2>&1); rc=$?
n=$(echo "$o" | grep -oE '^[0-9]+ standardised' | grep -oE '^[0-9]+')
say "the coil corpus is in standard form" "$( [ $rc = 0 ] && [ "${n:-0}" -ge 10 ] \
  && echo "ok  $n walks" || echo "FAIL rc=$rc, ${n:-no} walk(s) read")"

# THE TEN CHECK TRANSCRIPTS in coil/search/checks/ are an INPUT to a published
# page: gen_readme.js reads the "N checks, N failed" line out of each one and
# prints the total in README.md, which build.sh then renders into index.html.
# Nothing regenerated them. run_checks.sh writes them and no gate ran it, so a
# stale transcript would have gone on publishing a stale number for as long as
# nobody re-ran it by hand -- the same shape as every other artefact this file
# had to grow a gate for, in the one place where the artefact is a number in
# prose rather than a drawing.
#
# Regenerating them in place is the check: if any transcript has moved, the tree
# is dirty and the clean gate below says so. Verified current on 2026-09-10 by
# diffing all ten against live runs before this line existed.
#
# It costs about two and a half minutes, which is why the header above now says
# eight and not five.
cd $R/trumpet/parts/bore/concept/walk/no-elbows/coil/search
o=$(bash tools/run_checks.sh 2>&1); rc=$?
n=$(echo "$o" | grep -cE "[0-9]+ checks, [0-9]+ failed")
say "the search transcripts regenerate" "$( [ $rc = 0 ] && [ "$n" -ge 10 ] \
  && ! echo "$o" | grep -q 'NO SUMMARY' && echo "ok  $n walks" \
  || echo "FAIL rc=$rc, $n summary line(s)")"

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

