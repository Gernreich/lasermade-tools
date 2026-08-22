# lasermade-tools

Five scripts shared by the [LaserMadeMusic](https://www.youtube.com/@LaserMadeMusic)
build repositories — [torus-octagonal](https://github.com/Gernreich/torus-octagonal),
[trumpet-octagonal](https://github.com/Gernreich/trumpet-octagonal),
[trumpet-coiled](https://github.com/Gernreich/trumpet-coiled),
[trumpet-parts](https://github.com/Gernreich/trumpet-parts),
[bore-generator](https://github.com/Gernreich/bore-generator),
[knotwork-soundholes](https://github.com/Gernreich/knotwork-soundholes),
[living-hinge](https://github.com/Gernreich/living-hinge),
[slapstick](https://github.com/Gernreich/slapstick),
[kalimba](https://github.com/Gernreich/kalimba),
[bullroarer](https://github.com/Gernreich/bullroarer) and
[buzz-disc](https://github.com/Gernreich/buzz-disc).

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

Everything it checks is a claim the document makes about itself or about files on disk,
so a failure is always a real inconsistency — never a matter of taste. It reports:

- files the document names that do not exist, and shipped files nothing mentions
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
