# lasermade-tools

Six scripts shared by the [LaserMadeMusic](https://www.youtube.com/@LaserMadeMusic)
build repositories — [trumpet](https://github.com/Gernreich/trumpet),
[knotwork-soundholes](https://github.com/Gernreich/knotwork-soundholes),
[living-hinge](https://github.com/Gernreich/living-hinge),
[slapstick](https://github.com/Gernreich/slapstick),
[kalimba](https://github.com/Gernreich/kalimba),
[bullroarer](https://github.com/Gernreich/bullroarer) and
[buzz-disc](https://github.com/Gernreich/buzz-disc).

`trumpet` was five repositories until 2026-09-05 — `torus-octagonal`,
`trumpet-octagonal`, `trumpet-coiled`, `trumpet-parts` and `bore-generator`.
Notes below that name one of those are describing where a lesson came from, not
a repository you can clone.

**[The rest of the build files](https://gernreich.github.io/)** — every instrument,
generator and tool, indexed.

They exist because a writeup that tells someone how to cut wood can be wrong in ways
that cost them a sheet, and because a checker that reports the wrong thing is worse than
no checker at all. Every one of these has produced a wrong answer at some point. Each
section below says which, because that is the part worth remembering.

<!-- readme-only -->
**[Read the writeup](https://gernreich.github.io/lasermade-tools/)** — the same text as
this page, set for reading, with a table of contents.

Python 3, no dependencies. Nothing here reads or writes outside the paths you give it.

| | |
|---|---|
| `doc-audit.py` | claims a writeup makes about itself and about files on disk |
| `md2html.py` | markdown → one self-contained HTML page |
| `svg-stroke-check.py` | SVG elements whose stroke colour is declared twice and disagrees |
| `make-preview.py` | a cut file rendered so it can be read on a page |
| `test-ladder.py` | a strip of squares that finds the cut speed for the sheet in front of you |
| `flat-part-check.py` | the pre-cut gate for a flat single-sheet part: size, closure, cut order, holes |

## Why these live in their own repository

They are used by every repository above, so none of them can own the tools without all the
others depending on it. Tools that know one build — `torus-octagonal/verify_torus.js`
knows that build's apothems and panel sizes — stay inside the repository they describe.
These five know nothing about any particular object, so they sit here.

Until 2026-08-08 they lived in `~/Claude`, which is not version controlled. Their bugs
are the reason that mattered: the failures below were found by accident, and without
history there was no way to see when a check started lying.

---

## `doc-audit.py`

```
python3 doc-audit.py WRITEUP.md [--html PAGE.html]
        [--rebuild "python3 md2html.py {md} {out}"]
        [--links] [--run-blocks] [--ignore 'named,in,prose.py'] [--strict-h1]
```

A repository that legitimately names files it does not ship can say so once, in
**`.doc-audit-ignore`** at its root — one name per line, `#` for comments — instead of
remembering `--ignore` on every run. `torus-octagonal` needs it: its parts come from
**[boxes.py](https://www.festi.info/boxes.py/)** by Florian Festi, an external web generator, which serves
every download as `RegularBox.svg`.
Both names belong in the prose and neither will ever be a file there.

**A wrong answer it gave: `.json` read as `.js`.** The filename pattern accepted an
extension wherever it appeared rather than only at the end, so a document mentioning
`reduced.json` was reported as naming a missing `reduced.js`. It was unfixable from the
document's side — the file it named was right there — and it cost a false finding in
`spirals`, which mentions two `.json` artifacts. The extension must now end the name.
Real misses still fail: a document naming `ghost.js` with no such file still reports it.

Everything it checks is a claim the document makes about itself or about files on disk,
so a failure is always a real inconsistency — never a matter of taste. It reports:

- files the document names that do not exist, and shipped files nothing mentions
- files deleted from the index but left in the working tree — a retirement leftover,
  invisible to every check above because they all read `git ls-files`. Genuinely new
  untracked files are not reported: a cut file open in Inkscape is normal.
- link and image paths that do not resolve **from the document's own directory** — a
  separate question from the one above, and the one a browser actually asks
- dead in-page anchors, heading levels that skip, duplicate headings
- list counts that contradict the prose — "Three things" over two items
- doubled words, unbalanced code spans
- figures with no text alternative, print stylesheet, dark theme, language attribute
- whether the HTML is older than the markdown or than any SVG figure inlined into it
- `--run-blocks`: fenced blocks that look like terminal sessions, re-run and diffed
  against what the document claims they print. Any file a block writes is restored
- `--links`: external URLs actually resolve

File references in prose resolve against the **repository root**, not the document's
directory, so a document in a subdirectory can name files above it.

**Link and image targets are checked differently**, against the document's own directory,
because that is what a browser does with them. `see bell.py` in prose stays true wherever
bell.py lives; `<img src="bell.py">` does not. The two checks disagreeing is the point.

**Where it has been wrong.** Its first run produced four failures, all of which were its
own bugs rather than the document's. It also reported `boxes.py` and `RegularBox.svg` as
missing from `torus-octagonal` for as long as that repository existed — they are an
external tool and the filename it serves, correctly named in prose, and the fix was the
ignore file above rather than any change to a document that was right. Later, a document in a subdirectory was told its
neighbours did not exist, because the checker rooted everything at the document's own
directory; and the orphan check compared bare filenames, so `coupons/README.md` excluded
the *root* `README.md` from the pool and then reported files only that README mentions.

`.doc-audit-generated` at the root, one directory path per line, declares directories
whose contents are machine output. The orphan check then treats the directory as
documented rather than each file inside it. It exists for `bore-designs`, which is a
design library: seven directories of cut files, twenty-seven SVGs in one of them, and
naming every one in prose to satisfy a check would be worse writing, not better. It is
**declared, not inferred** — taking "this folder has its own README" as the signal was
the obvious alternative and would let any repository hide files from the check by
dropping a README into a directory. Two of the seven have no README anyway, so inference
would not even have worked.

Most recently the displayed-image check read a thumbnail as a missing picture. A page
showed a 214KB copy of a 1.4MB photograph and linked the original from it — which is what a
page should do, rather than making every reader pull the full file to see the picture — and
the check, which counted only `<img src>` and `![]()`, reported the original as named but
never shown. An anchor wrapping an image now counts as showing it. The check still fails a
photograph a document names and displays nowhere, which is the case it was written for.

The gallery rule added at the same time was wrong on its first push, in the opposite
direction: flex items default to `min-width: auto` and will not shrink below their
intrinsic width, so four bell renderings each took a row to themselves and the page got
longer rather than tidier. `min-width: 0` on the item is what makes them share a row. It
was caught by screenshotting the deployed page, which is the only thing that would have.

Before that `--run-blocks` failed a document that contains no fenced blocks at all. It
snapshotted the tree before running them, unconditionally, and `trumpet-curved` holds a
148MB video over the stash cap — which it then reported as a file a block "may have
modified". No block existed to modify it. The snapshot is now skipped when there is
nothing to run, and files over the cap are tracked by size and mtime, so the failure
states that a file *changed* rather than that one might have.

Before that it had no notion of a path at all. Every file reference, prose or image tag
alike, was satisfied by a name existing *anywhere* under the repository. So when
`trumpet-curved` moved its parts into subdirectories and nothing rewrote the paths, the
writeup passed **15/15** while 12 of the 13 images and 10 of the links on the published
page returned 404. Nothing local could see it; the only symptom was on the deployed site.
The path check above exists because of that, and it catches all 22 when replayed against
the commit that would have shipped them.

**Verify anything it flags against the source before acting on it.**

## `md2html.py`

```
python3 md2html.py WRITEUP.md PAGE.html
```

One self-contained page: no external CSS, no fonts, no scripts. SVG figures are **inlined
into the HTML**, which is why `doc-audit.py` treats a page older than any of its figures
as stale — regenerating an SVG does not change the page that already swallowed a copy of
it. Tab title comes from the document's own first `# ` heading.

Raw HTML blocks pass through unescaped, which is what makes the thumbnail-gallery tables
in these writeups render as tables rather than as visible source.

It had no branch for an **indented code block** — four leading spaces, CommonMark's
older fence-free form — so those lines fell through to the paragraph joiner and were run
together into prose. A design library with fifteen of them published its column-aligned
walk tables as `N N3 U1 N3 U1 N3 N y and z only 1 section, 0 elbows N N3 U2 …`, while
reading correctly on GitHub, which renders them.

`<!-- readme-only -->` on a line of its own drops the paragraph after it from the page
while leaving it in the README. It exists because a repository whose page is generated
from its own README published that README's "Read the writeup" line on the writeup —
a link to the page you were already reading, on seven pages. Marked rather than detected:
this converter knows only its input and output paths, and guessing the site URL from the
directory name breaks the moment a directory and its repository differ, as `test/` and
`bore-designs` do. GitHub renders the comment as nothing, so the README is unaffected.

**Where it has been wrong.** It escaped raw HTML blocks, so galleries appeared as their
own markup; and it had no blockquote branch, so `> ` lines rendered as literal text with
the marker showing.

## The checkers, audited against documents they should reject

Done 2026-09-08. Each check was given something it ought to fail.

**`flat-part-check.py` — all eight hold.** A file whose user unit is not a
millimetre, a part too big for the bed, a genuinely open cut path, an ink
outside the palette, a hole under the floor, a hole outside the outline, a hole
too near an edge, and a hole moved into the last cut stage — every one fails,
and the open path correctly drags `black frees the part` down with it.

Four probes came back clean and were wrong before the tool was: growing the
canvas is not growing the part; a path whose ends still meet is closed whether
or not it has a `z`; the near edge of a long thin blade is across it, not along
it; and the cut order is the ink, not the document order. **A check that does
not fire has not been tested until you know your input reached it.**

**`doc-audit.py` — effective, with two narrow blind spots.** A named file that
is not there, a heading level skipped, a stale page, a doubled word, a count
that contradicts its list, and a figure with no text alternative all fail.

- *no doubled words* is case-sensitive: `names names` is caught, `The the` is
  not — and a doubling across a sentence boundary is the common one.
- *"N things" matches the list under it* only counts **bold-lead** items. A
  claim over a plain list is not checked at all. It also reads any number
  followed by `checks` as a claim, which in these repositories is nearly always
  a measurement -- and its quoted-example guard fires only when a backtick
  abuts the matched words on BOTH sides, so backticking a longer phrase does
  nothing. Write `115 checks` exactly, or reword.

**What the figure check found.** An inlined SVG carries no `alt`, and
`md2html.py` was dropping it: the description written in the markdown reached
nothing, and a page was accessible only where the drawing happened to carry its
own `aria-label`. The two had already drifted — `living-hinge-guide.md` says
"x across the width" and `panel-convention.svg` says "x runs across the width".
`md2html.py` now uses the alt as the label when the SVG has none, and leaves the
drawing's own label alone where it has one. Every published page regenerates
byte-identical.

## `all-gates.sh`

Runs every gate in every repository and prints one tally.

    bash all-gates.sh
    ...
    GATES FAILING: 0

Fourteen ribbon bore runs, `regress.py` over 26 block designs, `doc-audit` over
every page in nine repositories, `svg-stroke-check` over every SVG, and
`flat-part-check` over the flat parts -- then four questions about what is
committed, and finally whether every repository is clean and pushed.

**The four are the ones a gate usually forgets**, because they compare the code
with what shipped rather than running the code at all:

    ribbon sheets reproduce byte-identical    40/40
    previews current with their cut files     40/40
    search tools reproduce their output
    Boxes install matches tools/

A harness that runs the generators and never diffs them against the artefacts
cannot tell you the sheets on disk are the sheets the code draws, which is this
project's entire claim. The last of the four matters most quietly: `bore_split`
shells out to the INSTALLED copy of `snakeboxvar.py`, so editing the one in
`tools/` and re-running changes nothing, silently.

About six minutes, most of it `regress.py`.

**It exists because of a failure of reading, not of checking.** Every one of
those gates already existed and every one was passing; what kept going wrong was
running them one at a time and not reading one of the answers -- a page
regenerated and its audit then run in a different repository, a tally printed
and pushed over. Nine gates run by hand are nine chances to read only eight.

### `repro-svg.py` — does a repository's shipped SVG still come out of its generator?

Both of trumpet's reproduction gates are trumpet-only, and every other
repository that ships generator-drawn cut files had none: knotwork-soundholes
draws 13, living-hinge 15, and nothing checked any of them.

They all reproduce. The finding was not drift — it was that **for four of the
knots the command that drew them was recorded nowhere**: not in the README, not
in the filename, not in a comment. `AMP` and `HW` do not scale with `R_HOLE`, and
the filename carries only leads, bights and radius, so every sample drawn at a
non-default radius was unreproducible from anything in the repository. They were
recovered on 2026-09-10 by reading the parameters back out of each SVG's own
description — the ribbon width is `2*HW`, the cosine amplitude is `AMP`, the rim
overrun is `BITE` — and each then matched byte for byte.

The manifest is `.repro` in the repository root, one line per shipped SVG:

    shipped/path.svg :: shell command writing the file to $OUT

A command of `!` means the file ships without a generator, deliberately, with
the reason in the comment above it. It still has to be listed — an SVG no line
claims is a failure, not a silence, which is how the 13 knotwork previews and
the hand-nested coupon sheet were found in the first place. Three things ship
under that marker and each was checked as far as it can be:

- **knotwork's previews** are filled even-odd in gold; no script in any
  repository draws that. Whatever made them is gone.
- **living-hinge's previews** have geometry identical to what `make-preview.py`
  draws today — 279 paths, same data — but a 0.6mm stroke where the tool now
  writes 0.4mm, so they predate a change to it.
- **the bridge-sweep coupon sheet** is a *cut* file with no generator, which is
  the case this marker should almost never cover. It holds 756 paths, exactly
  6 × 126, and every path of all six coupons is present in it by shape. So it is
  the six coupons and nothing else, and each of those six reproduces.

None of the three is a defect to fix silently: regenerating any of them chooses
a new look for pictures on a repository's front page, or re-nests a sheet by
hand. That is the author's call, so the gate records them and says so.

### What `doc-audit.py` got wrong on 27 documents

It had only ever been pointed at the nine documents that have a published page.
Aimed at the eighteen design notes as well, it produced 20 failures, of which
**one** was real. The four defects behind the other nineteen, all fixed
2026-09-10:

- **Indented code blocks were not stripped.** `strip_fences` knew about backtick
  fences only, so a gate's own quoted output four spaces in was read as prose.
  `393 checks, 0 failed` became a claim to be introducing a list that long --
  six false positives in one file.
- **Line numbers were reported against the stripped copy.** The balance check
  named lines 165 and 166 of a document whose wrapped span is on 178 and 179.
  Anything with a fenced block above the defect pointed at the wrong place.
  Stripped lines are now blanked, not deleted, which also stops a removed block
  making neighbours of two words that were never adjacent.
- **Code spans were balanced per line.** A span may wrap across a line break and
  close on the next; CommonMark renders it. Both of the only two things this
  check reported across 27 documents were that.
- **List items were counted only when bold-led.** "Two things had to be true"
  over two bullets reported *claimed 2, found 1*, because both opened with a
  code span -- and the 1 it found was a bold paragraph past the end of the list.
  Counting the wrong list is worse than counting none: the number looks measured.

A numeral before "checks" is now taken as a gate's assertion count rather than a
list claim. Across nine repositories every claim written in digits is one of
those and every genuine one spells the number out -- 17 instances, no
exceptions. `Three checks` over two items is still caught.

The real finding, for the record: **"Two things to know:" over three bullets**,
in the notes for the coil whose parts are cut.

**Every gate in it has been watched fail.** That is this repository's own rule
applied to the thing that enforces it, and it was not done when the script was
written. Appending one byte to a shipped cut file fires three of them at once
and nothing else:

    ribbon sheets reproduce byte-identical    FAIL 1 differ
    previews current with their cut files     FAIL 1 stale
    every repo clean                          FAIL 1 changed

Editing `parts.json` fires *search tools reproduce their output* and that alone,
so the gates are specific rather than cascading. *Boxes install matches tools/*
was watched fail by changing a default in the installed copy. *every repo
pushed* was proved in a scratch repository, since breaking it in a real one
means an unpushed commit. `doc-audit` and `flat-part-check` have failed
repeatedly in ordinary use.

The one exception is `regress.py`'s own verdict line, which has never been seen
to fail here -- that gate is inherited whole from the repository it checks, and
its history of catching things is recorded there rather than proved here.

**Where it has been wrong.** Its first run printed nothing at all for
`lasermade-tools`, `Gernreich.github.io` and `trumpet` -- the three most-edited
repositories -- because it paired `README.md` with `README.html` and those build
`index.html`. Nine repositories, six lines, and no complaint. A filter that
matches nothing reports a clean run over no files, which `check.py` had already
learned once and this then repeated.

## `svg-stroke-check.py`

```
python3 svg-stroke-check.py FILE.svg [FILE.svg ...]
python3 svg-stroke-check.py --dir DIR [--dir DIR] [--quiet]
python3 svg-stroke-check.py ... --fix
```

Finds SVG elements that declare their stroke colour twice, in two places that disagree —
a `style` property saying one colour and a presentation attribute saying another. The CSS
cascade resolves that to the style property, and browsers and Inkscape both agree, but
not every laser importer applies the cascade. One that reads the attribute instead puts
the part in a different cut stage.

**In these repositories colour is the cut order**, so that is not a cosmetic difference:
it is a part cut at the wrong moment, after the material holding it has already been
freed. Elements that declare the same colour twice are reported separately and are
harmless — no importer can disagree about them.

`--fix` deletes the redundant presentation attribute and keeps the style property that
was winning anyway, so what the file cuts does not change. It compares path data before
and after and refuses to write if anything but the attribute moved. Exit status is 1 when
conflicts exist, so it can gate a commit.

**Why it exists.** `torus-octagonal/BuildA1_90_25.svg` carried sixteen such paths: style
saying green or cyan, attribute saying black. Read the attribute way, all sixteen panels
— including the eight nested inside the plate holes — moved to the final stage, after the
cut that frees the plates. Nothing in the file looked wrong, and no other check could see
it.

## `flat-part-check.py`

```
python3 flat-part-check.py FILE.svg [FILE.svg ...]
python3 flat-part-check.py --dir DIR [--dir DIR] [--quiet]
python3 flat-part-check.py ... --bed 600x308 --min-edge 3 --min-hole 2 --weld 0.05
```

The bore repositories have a gate: `bore_split.py --write` checks every net it writes,
and nothing is cut from a file that has not passed. The flat parts — bullroarer blades,
buzz discs, anything cut from one sheet with holes in it — had no equivalent. This is it.

It reads geometry, not drawing. Paths, rects, circles, ellipses, polygons and lines are
flattened to polylines with every enclosing transform applied, and then measured:

| check | the mistake it is looking for |
|---|---|
| millimetre-true | a user unit that is not a millimetre cuts at 96/25.4 and looks right on screen the whole time |
| fits the bed | refused at the machine, or silently cropped |
| cut paths are closed | an outline that does not meet does not free the part |
| ink is in the palette | colour is the cut order here; an unknown colour is a stage nothing runs |
| black frees the part | the outline must be last, or the part moves while its holes are still being cut |
| holes cut before the outline | the same mistake, one stage at a time |
| holes are inside the outline | a hole on the waste, and usually a transform that was missed |
| hole is big enough | the cord holes are the point of these parts |
| edge distance | the cord hole is where a bullroarer fails — too little material and it tears out |

Exit status is 1 if any file fails, so it can gate a commit. `previews/` is skipped under
`--dir`: those are display renderings and are not cut.

**Kerf is not modelled.** Every measurement is of the path as drawn, and the beam takes
its width from both sides of that line — a 3.0mm hole cuts about 3.1mm, a 3.0mm wall comes
out about 2.9mm. Set `--min-edge` with that in hand rather than at the limit.

**Why it exists, and the two wrong answers it gave first.** Both were found by running it
on files known to be good, which is the only way this kind of bug surfaces.

It called `buzz-disc/BuzzDisc1.svg` three kinds of broken. The disc's outline carries no
`z` — Inkscape wrote the return point as an ordinary node, and the ends coincide to
0.00009mm, which cuts identically. Worse was what followed: with the outline discounted,
the largest closed path was a 5mm cord hole, so the gate reported that a cord hole freed
the part and that the *other* cord hole lay outside it. One wrong assumption, three
confident failures, all of them nonsense.

Then it called four of the five bullroarer blades open. They are drawn as a top curve and
a bottom curve meeting at the two tips: two open paths that cut one closed ring. Open
paths of the same colour are now stitched end to end within `--weld` before anything is
judged, and the report says how many joins it made.

The point of both: **a checker that reports the wrong thing is worse than no checker**,
and this one had to be run against known-good work before it was worth pointing at
anything else.

## `make-preview.py`

```
python3 make-preview.py CUTFILE.svg [OUT.svg]      # default previews/<name>
```

A cut file draws hairlines on no background: nearly invisible in a browser, and a black
one disappears on a dark page. This thickens the stroke, paints a light ground, and
darkens the inks that cannot be seen against it.

**It changes the colours, and that is the point.** Three of the six cut-order inks fail
the WCAG 3:1 graphics minimum on the cream ground — green at 1.28:1, cyan at 1.17:1,
orange at 2.35:1 — so a faithful rendering of the palette is an unreadable picture. The
darkened equivalents keep the hue and the sequence and clear 4.8:1. Geometry, sheet
position and cut order are untouched, and both are checked against the source before the
file is written.

The cut order it renders is shared by every build repository here: **blue engraves, then
green → orange → cyan → black**, black always the cut that frees the part, violet always
skip. A file uses only the stages it needs.

## `test-ladder.py`

```
python3 test-ladder.py OUT.svg [--speeds 15,20,25,30] [--square 15]
```

Settings for a sheet of plywood are a property of that sheet, not of the species on the
invoice. Baltic birch from two suppliers, or the same supplier two packs apart, will not
cut at the same speed, and the number that worked last month is a guess this month. This
draws four identical squares, each stroked in a different ink so the importer gives it its
own speed field, with its speed engraved beneath it in seven-segment line art — line art
rather than text, so no font substitution on import can turn a number into something else
or into nothing.

Cut it, push each square out from below, and the fastest one that drops free unaided is
the ceiling. Production runs 15-20% slower than that, so a void or a damp patch does not
cost a part.

**Colour here is one speed per layer, not cut order** — the only exception in these
repositories, and it is forced. xTool Studio and XCS split an import into processing
layers by colour, so a separate colour is the only place a separate speed can live. Same
ink would mean one layer and one speed: a row of identical squares and no ladder at all.

The inks are still the house stages, so `make-preview.py` renders a ladder and
`svg-stroke-check.py` reads it. Four rungs, because four stages actually cut — green,
orange, cyan, black — with blue engraving the labels as it always does. **Violet is never
emitted**, since it means skip and a skipped rung is a missing answer. Four is also the
better way to work: bracket coarse, then bisect. `15,25,35,45` finds the decade and a
second run across the winning pair finds the number, which lands closer than six rungs
guessed in one pass. Decimal speeds are accepted for that second run.

**Where it has been wrong.** Its first run crashed on every ladder it was asked to draw.
Speeds are parsed as floats, so `15` became `15.0`, and the digit table had no glyph for
`.` — a generator that had been described as validated could not render its own default
arguments. The decimal point is now a baseline tick, which is what makes bisecting below
whole numbers possible at all.

**The rungs themselves are untested against material.** The geometry is checked — square
count, ink uniqueness, label glyphs, parseable XML — and the ladder is known to survive
both sibling tools. Whether `15,20,25,30` brackets 3mm Baltic birch on a 55W tube is
exactly the question the ladder exists to answer, and nothing here has answered it yet.
Treat the default as a starting bracket, not a recommendation.

## Checking this page

`index.html` is this README rendered by `md2html.py` and committed, not built on the
server, so it goes stale silently unless it is regenerated after every edit:

```
python3 md2html.py README.md index.html
python3 doc-audit.py README.md --html index.html
```

The names that check would otherwise trip on now live in `.doc-audit-ignore` rather than
in an `--ignore` flag on the command line, so the documented self-check needs no arguments.
They are the usage-synopsis placeholders, and the files named in prose that live in the
repository they describe — `verify_torus.js` and `BuildA1_90_25.svg` in `torus-octagonal`,
`bell.py` nowhere at all, since it is the invented example in the `doc-audit.py` section
showing prose and image paths resolving differently. `bell.py` was missing from the flag
for as long as the flag existed, so the documented self-check reported two failures against
a README that was right.

Auditing this README is also what turned up the quoted-example bug fixed above: the line
describing the list-count check quotes `"Three things"`, and the checker read its own
example as a claim.

---

Released under [CC0 1.0](LICENSE).
