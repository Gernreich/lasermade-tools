---
name: writeup-review
description: Review a project writeup (markdown + generated HTML, a build guide, a README, a documentation page) for clarity, consistency and correctness using the thirteen lenses rather than repeated re-reads: eight that read a single document, five that compare a set of documents or repositories. Works on any project; LaserMadeMusic repositories also get their gate scripts. Use when asked to review, re-read, check or audit a writeup, build guide, documentation page, or a whole repo's docs — and when asked for "the seven lenses", which is what this was called before the set grew.
---

# Reviewing a writeup

A writeup that tells someone how to build, cut, install or run something can be
wrong in ways that cost them material or time. This skill is for finding those
before they do. It grew out of the LaserMadeMusic build guides and its examples
come from there, but the lenses are about documents, not lasers: they apply to
any project.

This file is version controlled in `lasermade-tools`, at
`skills/writeup-review/SKILL.md`; `~/.claude/skills/writeup-review` is a
symlink to that directory. Edit it there and commit.

## The one thing that matters most

**Re-reading the same artifact the same way stops paying almost immediately.**
In the review this came from, ten consecutive reads of the markdown found
progressively smaller style issues. Switching to the *rendered* text found a
step that named the wrong part, in the one operation where naming the wrong
part ruins the build. Then a lens that followed the instructions literally at a
different size found a script silently answering the wrong question.

So: **change the method, don't repeat the pass.** Each lens below looks for a
different class of defect. Run the ones that fit, report per lens, and stop a
lens when it stops paying rather than grinding it out.

Lenses 1–8 read one document. Lenses 9–13 compare documents against each other
and against the repository; reach for those when a project has grown more than
one document, or when files have been added since the prose was written.

## First, the mechanical half

Let scripts cover what needs no judgment, so the lenses spend their attention on
what does. Which scripts depends on where the document lives.

### In a LaserMadeMusic repository

The tools are their own repository, `~/LaserMadeMusic/GIT/lasermade-tools`
(github.com/Gernreich/lasermade-tools), beside the repositories they serve. They
were loose in `~/Claude` until 2026-08-08; a command that still says so is out of
date, not a missing file.

**Start with the whole harness**, not one script:

```
bash ~/LaserMadeMusic/GIT/lasermade-tools/all-gates.sh
```

About ten minutes, and it needs the network: the link gate fails rather than
skips without one. It runs every gate in every repository, `doc-audit.py` over
every markdown (paired with its page, with `--run-blocks`, and with `--links`),
`svg-stroke-check.py` over every SVG, `flat-part-check.py` over the flat parts,
`repro-svg.py` (does each shipped SVG still come out of its generator),
`name-check.py` (does a cut file's name agree with its geometry) and
`ignore-audit.py` (does every exemption still suppress something), then asks
whether every repository is clean and pushed. **Read the last lines.** It ends
`GATES FAILING: n` and names the failures again beneath it; it exits non-zero
when any gate fails. It exists because the recurring fault was not a bad check
but an unread one — nine gates run by hand are nine chances to read only eight.

Then, for the document under review, run the audit on its own to see the
detail the tally hides:

```
G=~/LaserMadeMusic/GIT/lasermade-tools
python3 $G/doc-audit.py WRITEUP.md --html PAGE.html \
    --rebuild "python3 $G/md2html.py {md} {out}" --run-blocks --links
```

Names the prose mentions but the repository does not ship belong in
`.doc-audit-ignore` at the repository root, and directories of machine output
in `.doc-audit-generated` — not in an `--ignore` flag retyped on every run.
Adding a line to either is a claim; `ignore-audit.py` is what tests it.

The tools' README records, per tool, the wrong answers each one has produced,
and which checks do not apply to which repositories (`flat-part-check.py` on a
rosette or a hinge, for instance). Read the section for any tool that flags
something before trusting the finding.

### Anywhere else

`doc-audit.py` needs nothing from LaserMadeMusic: Python 3, no dependencies, any
markdown file.

```
python3 ~/LaserMadeMusic/GIT/lasermade-tools/doc-audit.py README.md --links
```

Know its limits outside its home:

- The orphan check — tracked files nothing mentions — needs a git repository; it
  says so and skips otherwise.
- It only recognises file names ending `.svg .js .py .md .html .png .jpg .jpeg
  .css .zip`. A missing `build.sh`, `config.json` or `main.ts` goes unreported,
  so check those names by hand (lens 12).
- A list-count claim is only read in one shape: a capitalised number word up to
  Ten, then `things`, `pitfalls`, `rules`, `steps`, `reasons`, `ways`, `checks`
  or `traps` — "Three steps". Numerals are never read as list claims, so "the
  three stages", "Four options" and "5 steps" are yours to count.
- Leave out `--rebuild` and `--html` unless the page really is `md2html.py`
  output. Use the project's own build to regenerate the page, then compare the
  result with what is committed or deployed.
- `--run-blocks` executes the commands it recognises. Read the document's fenced
  blocks first and do not use it on a project whose commands deploy, delete or
  spend money.

Then use what the project already has: its tests, its linter, its link checker,
its build. A writeup whose quoted output no longer matches a live run is the
commonest stale claim there is, and the project's own commands are the way to
see it.

### Either way

For a repository whose SVGs are cut files, also run
`python3 $G/svg-stroke-check.py --dir . --quiet` (`all-gates.sh` already does
this for the LaserMadeMusic ones). It finds elements whose stroke colour is
declared twice and disagrees with itself — a `style` property and a
presentation attribute naming different colours. The cascade picks the style
property, and so do browsers and Inkscape, but an importer that reads the
attribute puts the part in a different cut stage. Where colour is the cut
order, that is a part cut at the wrong moment. Exit status is 1 when conflicts
exist; `--fix` drops the redundant attribute.

Anything a script flags is a real inconsistency — but **verify before reporting**. On
its first run `doc-audit.py` reported four failures that were all bugs in the
checker, not the document. Never report a finding you have not confirmed against
the source.

## The thirteen lenses — first, eight on one document

**1. Follow the build section literally.** Read only the section a reader would
follow to do the thing — the build steps, the install, the quick start — as if
the rest did not exist. Trace every instruction to a part, a file or a command.
Does each step name its inputs unambiguously? Are keep/discard instructions
explicit? Could a reader finish the section holding the wrong parts, or with
software in the wrong state? Where you can, actually run it from a clean
checkout rather than imagining it.

**2. Follow the general/parametric section at different numbers.** Actually
compute a second size end to end. If the document offers a parameter, an
option or a flag, *use* it — tools often only implement the worked example.
Check that every formula, table and helper script honours the parameter the
prose says you may change.

**3. Adversarial misreading.** For each instruction, ask what a hurried reader
would do. Watch for imperatives that appear *before* their preconditions —
"cut it as-is" above the two things you must do first is the archetype.

**4. Numbers, recomputed from scratch.** Derive every figure from first
principles in a script, copying nothing from the document, then assert the
document contains each result. Catches rounding drift and stale values that
survive any amount of reading. Watch for two conventions in one table (before
and after kerf; nominal and as-drawn; with and without a default applied).

**5. External references.** Fetch every link. Confirm quoted third-party text
against the actual source rather than memory.

**6. Accessibility and print.** Heading order, alt text, tables that overflow,
print stylesheet, dark theme, language attribute. Mostly the script's job; read
the result, then confirm it with lens 8 — the script cannot see a layout.

**7. Read it backwards.** Last section first. Exposes forward references and
assumed context that reading in order glides over.

**8. Look at the published page.** Screenshot the deployed URL in a browser —
not the local file, so the generator, the host and the cache are all in the
path. Check it on a narrow width too, and on whichever theme you do not use
yourself. If the document is only ever read where it is hosted — a README on
GitHub, a page in a docs site — look at it there, since that renderer is the
one with the quirks.

```
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless \
  --disable-gpu --hide-scrollbars --window-size=1100,1600 \
  --virtual-time-budget=9000 --screenshot=/tmp/page.png 'URL?v=cachebust'
```

**Narrow width cannot be done by shrinking `--window-size`.** Headless Chrome
lays the page out at least 500px wide and then crops the screenshot to the size
you asked for, so at 390px every page looks clipped on the right, even one that
fits perfectly. *Produced a false "overflows on phones" on six pages at once;
a control page printing `innerWidth` showed 500 in a 390 window.* Instead, frame
the page at phone width inside a window the browser honours:

```
cat > /tmp/probe.html <<EOF
<!doctype html><body style="margin:0"><iframe src="URL?v=cachebust"
  style="width:400px;height:900px;border:0"></iframe></body>
EOF
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless \
  --disable-gpu --hide-scrollbars --window-size=500,900 \
  --virtual-time-budget=12000 --screenshot=/tmp/narrow.png file:///tmp/probe.html
```

The iframe's viewport really is 400px, so what is cut off there is cut off on a
phone. For the other theme, add `--force-dark-mode
--blink-settings=preferredColorScheme=0` to the first command.

This is the only lens that sees the **composition**, and that is a real category:
every layer can be individually correct and the result still wrong, so nothing
that reads markup will ever flag it. *Caught a warning callout rendering as
literal "> " text, because the converter had no blockquote branch — the markdown
was right and the HTML was valid. Caught gallery captions sitting left of their
images despite `align="center"`, because a stylesheet rule beats a presentation
attribute. Caught a preview that was well-formed, passed every geometric
invariant, and drew a shape that did not exist.*

Two ways to fool yourself here. **Cache-bust the URL**, or you will review a
page you already fixed. And **confirm the deploy that finished is the one you
pushed** — matching on "most recent run" once told me a build had landed when
what completed was the previous commit's.

## Then five more, across the set of documents

The eight above read one document. These compare documents with each other and
with the repository, and they find a different class of defect: not a wrong
statement, but two statements that cannot both be true. Run them when a project
has more than one document, or when files have been added since the prose was
written. Each one below is credited with what it actually caught.

**9. The same fact, stated twice.** List every figure that appears in more than
one file — a README, a landing page, a writeup — and check they agree, and that
each still matches a live run. *Caught: a landing page claiming "49% to 63% of
the disc removed across the documented variants" when the tables ran 38.2% to
63.3%, and naming a default that was not the default. It sat in a "Before you
cut" warning, where understating removal is the dangerous direction.*

**10. Absolutes and superlatives.** Grep for `the only`, `the one … that`,
`the smallest`, `the fewest`, `never`, `always`, `no other`. These are true when
written and go stale the moment a file is added — and the document rarely gets
re-read at the point that happens. Test every one you can. *Caught three in one
pass: "the one sample not at 30mm" (contradicted by the two bullets beneath it),
"the smallest panel on which five leads resolve" (a smaller one resolved fine),
and "the only nine-lead result that passes" (another did too). Also confirmed
two absolutes as correct, which is worth saying rather than silently leaving.*

**11. Terminology and units.** One concept, one word; one unit style. Count
competing spellings (`colour`/`color`), competing compounds
(`cut-out`/`cutout`), and unit spacing (`30mm`/`30 mm`) per file *and* across
files. Two documents in one project disagreeing reads as carelessness even when
every number is right. **Protect fenced code blocks and any string the tools
emit** — rewriting those makes the document misquote its own output, and breaks
`--run-blocks`. If the generators emit the losing style, fix it at the source
and regenerate rather than leaving the docs quoting something that no longer
exists.

**12. File lists, both directions.** Every shipped file named somewhere, and
every named file shipped — the script does this for the extensions it knows,
and the rest is by hand. Check that names are given *in full*. A table that writes `...-bridge1.svg` documents nothing a reader can copy
and nothing a checker can resolve.

**13. Cross-repo parity.** Sibling projects should present themselves the same
way. Build a matrix: download link, video/channel link, published-page link,
repository link, licence, file table — or, for software, install command,
supported versions, usage example, changelog, licence. A gap is usually an omission rather than a
decision. *This one found nothing broken — which is a result, and worth one line
in the report rather than silence.*

## How to report

- Findings per lens, most consequential first. Say plainly when a lens found
  nothing — that is information, not failure.
- Fix what is unambiguously wrong. **Flag judgment calls instead of acting**:
  restructuring, tone, anything the author chose deliberately.
- Distinguish "would mislead someone at the machine" from "style". Do not
  inflate the second into the first.
- If a change is large (rewriting a generator, renumbering a document), propose
  it and wait.

## The checking script is under review too

Lenses 2, 4, 9 and 10 all mean writing a throwaway script, and a bad one reports
success. Two failures worth designing against, both from real passes:

- **A check that parses nothing passes.** A cross-document comparison whose two
  regexes both matched zero rows printed "0 mismatches" and looked like a clean
  result. **Assert you parsed what you expected** — `assert len(rows) == 6` —
  and print the count alongside the verdict, so an empty comparison cannot
  masquerade as agreement.
- **Geometry parsers mis-pair coordinates.** Flattening an SVG path's numbers
  and taking every second one as `y` breaks the moment a path uses `H`, `V` or
  an arc, because the run length stops being even. This produced three separate
  false findings in one session — a "clipped" file, and twice a measurement that
  contradicted a document that was right. Parse per command, or measure with the
  tool that wrote the file.

When a check contradicts a document, suspect the check first. In this skill's
history that has been the right bet more often than not.

## Watch your own edits

A striking share of defects in the original review were introduced by the
reviewer's own immediately preceding edit: a false instruction, a heading that
swallowed the section under it, a cross-reference in a style used nowhere else.

After any edit that adds or moves a heading, **dump the document outline** and
confirm the content still sits under the heading it belongs to. After any edit
that adds a claim about a file or a tool, **test that claim**.

Re-run the mechanical audit after every change, not just at the end.
