# lasermade-tools

Four scripts shared by the [LaserMadeMusic](https://www.youtube.com/@LaserMadeMusic)
build repositories — [torus-octagonal](https://github.com/Gernreich/torus-octagonal),
[trumpet-octagonal](https://github.com/Gernreich/trumpet-octagonal),
[trumpet-curved](https://github.com/Gernreich/trumpet-curved),
[knotwork-soundholes](https://github.com/Gernreich/knotwork-soundholes),
[living-hinge](https://github.com/Gernreich/living-hinge),
[slapstick](https://github.com/Gernreich/slapstick),
[kalimba](https://github.com/Gernreich/kalimba),
[bullroarer](https://github.com/Gernreich/bullroarer) and
[buzz-disc](https://github.com/Gernreich/buzz-disc).

They exist because a writeup that tells someone how to cut wood can be wrong in ways
that cost them a sheet, and because a checker that reports the wrong thing is worse than
no checker at all. Every one of these has produced a wrong answer at some point. Each
section below says which, because that is the part worth remembering.

Python 3, no dependencies. Nothing here reads or writes outside the paths you give it.

| | |
|---|---|
| `doc-audit.py` | claims a writeup makes about itself and about files on disk |
| `md2html.py` | markdown → one self-contained HTML page |
| `svg-stroke-check.py` | SVG elements whose stroke colour is declared twice and disagrees |
| `make-preview.py` | a cut file rendered so it can be read on a page |

## Why these live in their own repository

They are used by all nine build repositories, so none of them can own the tools without
the other eight depending on it. Tools that know one build — `torus-octagonal/verify_torus.js`
knows that build's apothems and panel sizes — stay inside the repository they describe.
These four know nothing about any particular object, so they sit here.

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

Everything it checks is a claim the document makes about itself or about files on disk,
so a failure is always a real inconsistency — never a matter of taste. It reports:

- files the document names that do not exist, and shipped files nothing mentions
- dead in-page anchors, heading levels that skip, duplicate headings
- list counts that contradict the prose — "Three things" over two items
- doubled words, unbalanced code spans
- figures with no text alternative, print stylesheet, dark theme, language attribute
- whether the HTML is older than the markdown or than any SVG figure inlined into it
- `--run-blocks`: fenced blocks that look like terminal sessions, re-run and diffed
  against what the document claims they print. Any file a block writes is restored
- `--links`: external URLs actually resolve

File references resolve against the **repository root**, not the document's directory, so
a document in a subdirectory can name files above it.

**Where it has been wrong.** Its first run produced four failures, all of which were its
own bugs rather than the document's. Later, a document in a subdirectory was told its
neighbours did not exist, because the checker rooted everything at the document's own
directory; and the orphan check compared bare filenames, so `coupons/README.md` excluded
the *root* `README.md` from the pool and then reported files only that README mentions.
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

## Checking this page

```
python3 doc-audit.py README.md --ignore 'WRITEUP.md,PAGE.html,FILE.svg,CUTFILE.svg,OUT.svg,prose.py,verify_torus.js,BuildA1_90_25.svg'
```

The ignore list is the usage-synopsis placeholders and two files that live in
`torus-octagonal`, not here. Auditing this README is what turned up the quoted-example
bug fixed above: the line describing the list-count check quotes `"Three things"`, and
the checker read its own example as a claim.

---

Released under [CC0 1.0](LICENSE).
