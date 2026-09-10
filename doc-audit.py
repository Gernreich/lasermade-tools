#!/usr/bin/env python3
"""Mechanical audit for a markdown writeup and the HTML generated from it.

    python3 ~/LaserMadeMusic/GIT/lasermade-tools/doc-audit.py WRITEUP.md
             [--html PAGE.html]
             [--rebuild "python3 ~/LaserMadeMusic/GIT/lasermade-tools/md2html.py {md} {out}"]
                                  [--links] [--run-blocks]

Project-agnostic. Everything it checks is a claim the document makes about
itself or about files on disk, so a failure is always a real inconsistency.
Judgment-level review (does this instruction mislead? is the order safe?) is
not here — that is the reading half, see the writeup-review skill.
"""
import argparse, html as H, os, re, subprocess, sys, tempfile, pathlib, urllib.request

ap = argparse.ArgumentParser()
ap.add_argument("md")
ap.add_argument("--html")
ap.add_argument("--rebuild", help='command template with {md} and {out}')
ap.add_argument("--links", action="store_true", help="check external URLs resolve")
ap.add_argument("--run-blocks", action="store_true",
                help="re-run fenced blocks that look like terminal sessions and diff; "
                     "any file a block writes is restored afterwards")
ap.add_argument("--ignore", default="", help="comma-separated filenames named in prose, not shipped")
ap.add_argument("--ignore-file", default="", help="use this instead of the repository's .doc-audit-ignore")
ap.add_argument("--strict-h1", action="store_true",
                help="require exactly one <h1>; off by default, since a chaptered document may use several")
a = ap.parse_args()

MD = pathlib.Path(a.md)
# Resolve file references against the repository root, not the document's own
# directory. A document that sits in a subdirectory (coupons/README.md) legitimately
# names files one level up, and its siblings are documented by the writeups above it;
# rooting at MD.parent reported both as failures. For a document at the repo root
# this is the same directory, so nothing else changes.
ROOT = MD.parent
try:
    _top = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=MD.parent or ".",
                          capture_output=True, text=True, check=True).stdout.strip()
    if _top:
        ROOT = pathlib.Path(_top)
except Exception:
    pass
# git ls-files reports paths relative to the repository root, so identify the document
# the same way. Comparing on the bare name made coupons/README.md exclude the *root*
# README from the pooled documents and then treat its own siblings as undocumented.
try:
    MD_REL = MD.resolve().relative_to(ROOT.resolve()).as_posix()
except ValueError:
    MD_REL = MD.name
src = MD.read_text()
page = pathlib.Path(a.html).read_text() if a.html else None
fails, notes = [], []


def strip_fences(text):
    """Everything inside a ``` block is quoted content, not markdown. A shell comment
    there is not a heading — the octagonal torus and celtic knot writeups both quote
    sessions containing '# ...' lines, which the renderer ignores and a naive regex
    reads as an <h1>. Same toggle rule md2html.py uses, so the two agree on what is
    code. Filename mentions are deliberately NOT stripped: `node foo.js` inside a
    block is a real reference to foo.js."""
    out, inblock = [], False
    lines = text.split("\n")
    # An INDENTED code block is quoted content too, and was not being stripped:
    # tools/CLAUDE.md quotes a gate's own output four spaces in, and the counts
    # check below read "393 checks, 0 failed" as a claim to be listing 393
    # things. Six false positives in one document, in a file nothing gated --
    # so the bug survived because the doc was never audited, not because the
    # check was sound.
    #
    # CommonMark: an indented block needs a blank line before it and cannot
    # interrupt a paragraph. Four spaces under a LIST is list continuation, not
    # code, so a run is only treated as a block when the last ordinary line
    # above it was neither a list item nor itself indented. Getting this wrong
    # in the loose direction would silently blind every check below to real
    # prose, which is the more expensive mistake.
    incode, prev = [False] * len(lines), ""
    for i, ln in enumerate(lines):
        if ln.strip() and not ln.startswith((" " * 4, "\t")):
            prev = ln
            continue
        if not ln.strip():
            continue
        j = i - 1
        while j >= 0 and not lines[j].strip():
            j -= 1
        opens = j < 0 or (not lines[j].startswith((" " * 4, "\t"))
                          and lines[j].strip() == prev.strip()
                          and not re.match(r"\s*(?:[-*+]|\d+\.)\s", prev))
        if opens or (i and incode[i - 1]):
            incode[i] = True
    # Stripped lines are BLANKED, not deleted, so every line number the checks
    # below report is the line number in the file. They were not: the balance
    # check pointed at 165 and 166 of a document whose wrapped code span is on
    # 178 and 179, because it was counting lines in the stripped copy. Anything
    # with a fenced block above the defect reported the wrong place, and the
    # bigger the quoted output the further off it was. Blanking also stops a
    # removed block making neighbours of two words that were never adjacent --
    # the same fault the \x00 placeholder fixes for code spans on 2026-09-09.
    for i, ln in enumerate(lines):
        if ln.startswith("```"):
            inblock = not inblock
            out.append("")
            continue
        out.append("" if inblock or incode[i] else ln)
    return "\n".join(out)


prose_src = strip_fences(src)


def ok(label, good, detail=""):
    (notes if good else fails).append(f"{'  ✓' if good else '  ✗'} {label}{'  ' + detail if detail else ''}")


# ── 1. files the document names, and files nobody names ──────────────────────
prose_only = {x.strip() for x in a.ignore.split(",") if x.strip()}
# A repository may legitimately name files it does not ship. torus-octagonal is built
# with boxes.py, an external web generator, and explains that it serves every download
# as RegularBox.svg -- so both names belong in the prose and neither will ever exist on
# disk. Passing --ignore each time works until someone forgets, and then the same two
# false failures come back looking like a regression. A repository states its own
# exceptions once, in .doc-audit-ignore at the root: one name per line, # for comments.
# --ignore-file points that somewhere else, so the exemption list can be audited
# against a COPY with one line removed rather than by editing the repository's
# own file in a loop. Every entry is a standing claim -- "this name is prose,
# not a file" -- and a claim nothing re-tests is one that quietly stops being
# true; but proving that must not be able to damage what it is testing.
IGNORE_FILE = pathlib.Path(a.ignore_file) if a.ignore_file else ROOT / ".doc-audit-ignore"
if IGNORE_FILE.exists():
    for ln in IGNORE_FILE.read_text().split("\n"):
        ln = ln.split("#", 1)[0].strip()
        if ln:
            prose_only.add(ln)

# Some repositories are mostly machine output. A design library holds a directory per
# design - twenty-seven cut files in one of them - documented collectively, and naming
# every SVG in prose to satisfy the orphan check would be worse writing, not better.
# A repository declares those directories once, in .doc-audit-generated at the root:
# one path per line, # for comments. Declared rather than inferred on purpose - taking
# "this folder has its own README" as the signal would let any repository hide files
# from the check by dropping a README into a directory.
GENERATED_FILE = ROOT / ".doc-audit-generated"
generated_dirs = []
if GENERATED_FILE.exists():
    for ln in GENERATED_FILE.read_text().split("\n"):
        ln = ln.split("#", 1)[0].strip().rstrip("/")
        if ln:
            generated_dirs.append(ln + "/")
no_urls = re.sub(r'https?://\S+', ' ', src)          # a URL's tail is not a local file
# The extension must end the name. Without the lookahead, ".json" matched ".js" inside
# it, so a document mentioning reduced.json was reported as naming a missing reduced.js --
# a phantom the reader cannot fix, because the file it names is right there.
refs = sorted(set(re.findall(
    r'[A-Za-z0-9_][A-Za-z0-9_.-]*\.(?:svg|js|py|md|html|png|jpg|jpeg|css|zip)(?![A-Za-z0-9])',
    no_urls)))
# A document may name a file that lives in a subdirectory — "coupons/sweep.svg" or
# just "sweep.svg" in a listing. Accept either: exists at ROOT, or exists anywhere
# below it under that name. Only a name matching nothing is a real dangling reference.
def somewhere(name):
    if (ROOT / name).exists():
        return True
    base = pathlib.PurePath(name).name
    return any(q.name == base for q in ROOT.rglob(base) if ".git" not in q.parts)


missing = [r for r in refs if not somewhere(r) and r not in prose_only]
ok("every file the document names exists", not missing, str(missing) if missing else "")

# A name and a path are different claims, and only one of them is a URL.
#
# The check above is deliberately loose: prose that says "generated by bell.py" is a
# reference, not a link, and it stays true wherever bell.py lives. An <img src> is the
# opposite — a browser resolves it against the page's own directory and looks nowhere
# else. Treating both the same way passed trumpet-curved 15/15 while every image on the
# deployed page 404'd, because the files had moved into subdirectories and nothing
# rewrote the paths. Nothing local could see it; only fetching the published page could.
#
# So: resolve link and image targets from the DOCUMENT's directory, not from ROOT.
LINK_TARGET = re.compile(
    r'<img\b[^>]*?\ssrc="([^"]*)"'
    r'|<a\b[^>]*?\shref="([^"]*)"'
    r'|!\[[^\]]*\]\(\s*<?([^)>\s]+)'
    r'|(?<!!)\[[^\]]*\]\(\s*<?([^)>\s]+)')
SKIP_SCHEME = ("http://", "https://", "data:", "mailto:", "tel:", "//", "#", "{")


def link_targets(text):
    for m in LINK_TARGET.finditer(text):
        u = next(g for g in m.groups() if g is not None)
        u = u.split("#", 1)[0].split("?", 1)[0].strip()
        if u and not u.startswith(SKIP_SCHEME):
            yield u


unresolved = sorted({u for u in link_targets(src)
                     if not (MD.parent / u).exists() and u not in prose_only})
ok("every link and image path resolves from the document", not unresolved,
   str(unresolved) if unresolved else "")

try:
    tracked = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True,
                             check=True).stdout.split()
    # "Somewhere" is the whole repository, not just the document being audited: a repo may
    # carry several writeups plus a README, and a file named in any of them is documented.
    # Pooling them stops each document reporting its siblings' files as orphans.
    docs = src
    for t in tracked:
        if t.endswith(".md") and t != MD_REL:
            try:
                docs += (ROOT / t).read_text()
            except OSError:
                pass
    # A generated page is the rendering of its markdown, not a separate artifact to name.
    generated = {t for t in tracked if t.endswith(".html") and t[:-5] + ".md" in tracked}
    # README.md needs no introduction for the same reason LICENSE does not, and anything
    # under a dot-directory (.github/workflows/…) is repository plumbing, not content.
    infra = {t for t in tracked if any(p.startswith(".") for p in t.split("/"))}
    # Contents of a declared generated directory are documented by the directory, not
    # one file at a time.
    machine = {t for t in tracked if any(t.startswith(d) for d in generated_dirs)}
    skip = ({"LICENSE", ".gitignore", "README.md", MD_REL} | generated | infra | machine
            | ({pathlib.Path(a.html).name} if a.html else set()))
    # A tracked file counts as mentioned whether the prose gives its full path or just
    # its name — a listing of coupon filenames documents coupons/ as surely as a path would.
    def named(f):
        return f in docs or pathlib.PurePath(f).name in docs

    orphans = [f for f in tracked if not named(f) and f not in skip]
    ok("every tracked file is mentioned somewhere", not orphans, str(orphans) if orphans else "")

    # A file deleted from the index but left sitting in the working tree is invisible
    # to every check above, all of which read `git ls-files`. It is not new work either
    # — git has seen it before. That is what a size retirement leaves behind: two 25mm
    # mouthpiece sheets survived trumpet-parts' 10mm-only rewrite for a day, documented
    # nowhere and flagged by nothing. Genuinely new untracked files are not reported,
    # because a cut file open in Inkscape is normal and would drown the signal.
    others = subprocess.run(["git", "ls-files", "--others", "--exclude-standard"],
                            cwd=ROOT, capture_output=True, text=True).stdout.split()
    if others:
        seen = set(subprocess.run(
            ["git", "log", "--all", "--diff-filter=D", "--name-only", "--format="],
            cwd=ROOT, capture_output=True, text=True).stdout.split())
        left = sorted(f for f in others if f in seen
                      and f not in prose_only
                      and pathlib.PurePath(f).name not in prose_only)
    else:
        left = []
    ok("no file deleted from the index is still on disk", not left,
       str(left) if left else "")

    # A named file and a shown file are different things. Twice in one project a photograph
    # was listed in a README's file table and displayed nowhere, so the document most people
    # see first named a picture it never showed. The orphan check above passes on a mention;
    # this one asks whether the image is actually on the page.
    RASTER = (".jpg", ".jpeg", ".png", ".gif", ".webp")
    # Per document, not pooled. Pooling was the first attempt and it could not catch the
    # case it was written for: the photograph WAS shown, in the writeup, while the README
    # that named it showed nothing. An image a document names is an image that document
    # should show.
    shown = {m.group(1).split("/")[-1] for m in re.finditer(r'<img[^>]+src="([^"]+)"', src)}
    shown |= {m.group(1).split("/")[-1] for m in re.finditer(r'!\[[^\]]*\]\(([^)\s]+)', src)}
    # A full-size photograph offered behind its own thumbnail is shown, not merely named.
    # Counting only <img src> failed a page that displayed a 214KB copy of a 1.4MB
    # original and linked the original from it, which is exactly what a page should do
    # rather than making every reader download the full file to see the picture.
    shown |= {m.group(1).split("/")[-1]
              for m in re.finditer(r'<a[^>]+href="([^"]+)"[^>]*>\s*<img', src)}
    named_here = {r.split("/")[-1] for r in refs if r.lower().endswith(RASTER)}
    unshown = sorted(n for n in named_here if n not in shown and n not in prose_only)
    ok("every image is displayed, not just named", not unshown,
       str(unshown) if unshown else "")
except Exception:
    notes.append("  – not a git repo, skipped the orphan check")

# ── 2. in-page anchors ───────────────────────────────────────────────────────
def slug(t):
    """GitHub's rule: drop anything that is not a letter, digit, space or hyphen,
    then turn spaces into hyphens. An em dash therefore leaves a double hyphen."""
    t = re.sub(r"`|\*\*|\*", "", t)
    return re.sub(r"\s", "-", re.sub(r"[^a-z0-9 \-]", "", t.lower()))


heads = re.findall(r"(?m)^(#{1,6})\s+(.*)$", prose_src)
ids = {slug(t) for _, t in heads}
dangling = sorted({m for m in re.findall(r"\]\(#([a-z0-9-]+)\)", prose_src)} - ids)
ok("in-page links resolve to a heading", not dangling, str(dangling) if dangling else "")

# ── 3. heading structure ─────────────────────────────────────────────────────
lv = [len(h) for h, _ in heads]
skips = [(lv[i], lv[i + 1]) for i in range(len(lv) - 1) if lv[i + 1] - lv[i] > 1]
ok("heading levels never skip a level", not skips, str(skips) if skips else "")
dupes = {t for _, t in heads if [x for _, x in heads].count(t) > 1}
ok("no two headings share a title", not dupes, str(sorted(dupes)) if dupes else "")

# ── 4. counts the prose claims about its own lists ───────────────────────────
WORDS = {"one": 1, "two": 2, "three": 3, "four": 4, "five": 5, "six": 6,
         "seven": 7, "eight": 8, "nine": 9, "ten": 10}
for m in re.finditer(r"\b(One|Two|Three|Four|Five|Six|Seven|Eight|Nine|Ten|\d+)\s+"
                     r"(things|pitfalls|rules|steps|reasons|ways|checks|traps)\b", prose_src):
    claimed = WORDS.get(m.group(1).lower(), None) or int(m.group(1))
    # A document that quotes the pattern as an example is not making the claim.
    # This checker's own README says: list counts that contradict the prose —
    # "Three things" over two items. Read as a claim, that counted the bold leads
    # in the section below it and reported a mismatch against a sentence that was
    # only describing the check. Quotation marks and backticks mean "mentioned".
    before = prose_src[m.start() - 1: m.start()]
    after = prose_src[m.end(): m.end() + 1]
    if before in '"“`' and after in '"”`':
        continue
    # A NUMERAL before "checks" is a gate reporting how many assertions it ran,
    # not prose introducing a list. Across the nine repositories every claim
    # written as digits is one of those -- 195, 226, 393, 7700 checks -- and
    # every genuine list claim spells the number out: "Two things", "Three
    # ways". 17 instances, no exceptions. Read as claims, the digits produced
    # six false positives in tools/CLAUDE.md alone and never once caught a real
    # miscount. "Three checks" over two items is still checked.
    if m.group(2) == "checks" and m.group(1).isdigit():
        continue
    # Count the list that FOLLOWS the claim, and count every item in it -- not
    # only the ones with a bold lead. "Two things had to be true" over two
    # bullets was reported as claimed 2, found 1, because both bullets opened
    # with a code span instead of bold and the bold-lead pattern matched neither;
    # the 1 it did find was a bold paragraph further down, past the end of the
    # list. Counting the wrong list is worse than counting nothing, because the
    # number it prints looks like a measurement.
    tail = re.split(r"(?m)^#{1,6}\s", prose_src[m.end():])[0]
    items, seen = 0, False
    for ln in tail.split("\n"):
        if re.match(r"\s*(?:\d+\.|[-*+])\s", ln):
            items, seen = items + 1, True
        elif seen and not ln.strip():
            continue            # a blank line between items keeps the list open
        elif seen and not ln.startswith((" ", "\t")):
            break               # unindented prose ends the list
    if not items:  # some lists are bold-lead paragraphs rather than markdown list items
        items = len(re.findall(r"(?m)^\*\*[^*]+\*\*", tail))
    if items:
        ok(f'"{m.group(0)}" matches the list under it', items == claimed,
           f"claimed {claimed}, found {items}")

# ── 5. prose hygiene ─────────────────────────────────────────────────────────
prose = prose_src
prose = re.sub(r"https?://\S+", "", prose)
# A code span is a quotation, not prose. The counts check above already works
# this way -- "Quotation marks and backticks mean mentioned" -- and this one did
# not, so a document that quotes `names names` as an EXAMPLE of a doubled word
# was reported as containing one. Found by this file's own audit section doing
# exactly that on 2026-09-09.
# A NON-SPACE placeholder, and it took three goes to get right. Removing the
# span outright made neighbours of words that were never adjacent -- "and
# `-w80` and" read as a doubled "and". Substituting a space did not help,
# because the doubled-word pattern spans any run of whitespace and a line like
# "`a` and `b` and `c` all work" still collapsed to "and   and". Only a
# character that is neither word nor space breaks the pair. All three states of
# this were introduced and caught on 2026-09-09, by the check itself, on prose
# written to document something else.
spanless = re.sub(r"`[^`]*`", "\x00", prose)
dbl = [m.group(1) for m in re.finditer(r"\b([A-Za-z]{3,})\s+\1\b", spanless)]
ok("no doubled words", not dbl, str(dbl) if dbl else "")
# The BALANCE check reads the original, not the stripped copy. Substituting a
# span that runs across a line break joins those lines, which shifted every
# line number after it -- a stray backtick on line 6 was reported as line 5 --
# and it also swallowed the very backticks this check exists to count. Only the
# doubled-word test above wants spans gone.
#
# Balanced per PARAGRAPH, not per line. A code span may wrap across a line break
# and CommonMark closes it in the next line -- `plate_span_mm()` sums ... it used
# to be `blocksize x\nblocks` renders correctly and read as two broken lines.
# Two of those were the only thing this check reported across 27 documents, so
# per-line was finding nothing real and hiding behind noise. A span that never
# closes leaves its paragraph odd, which is the defect worth reporting, and the
# line given is where the paragraph starts.
para, start, odd = 0, 1, []
for i, l in enumerate(prose.split("\n") + [""]):
    if l.strip():
        if not para:
            start = i + 1
        para += l.count("`")
    elif para % 2:
        odd.append(start)
        para = 0
    else:
        para = 0
ok("code spans balanced in every paragraph", not odd,
   f"paragraphs starting at {odd}" if odd else "")

# ── 6. the generated page ────────────────────────────────────────────────────
if page:
    hl = [int(x) for x in re.findall(r"<h([1-6])", page)]
    if a.strict_h1:
        ok("page has exactly one <h1>", hl.count(1) == 1, f"found {hl.count(1)}")
    elif hl.count(1) > 1:
        notes.append(f"  – {hl.count(1)} <h1> elements; fine for a chaptered page, "
                     f"pass --strict-h1 if you want one")
    pids = set(re.findall(r'id="([^"]+)"', page))
    plinks = set(re.findall(r'href="#([^"]+)"', page))
    ok("no broken anchors in the page", not (plinks - pids), str(sorted(plinks - pids)))
    figs = re.findall(r"<figure>(.*?)</figure>", page, re.S)
    bad = [i + 1 for i, f in enumerate(figs)
           if ("<svg" in f and "aria-label" not in f) or ("<img" in f and not re.search(r'alt="[^"]+"', f))]
    ok("every figure has a text alternative", not bad, f"figures {bad}" if bad else "")
    ok("wide tables scroll inside their own box",
       page.count("<table>") == 0 or "overflow-x:auto" in page)
    ok("print stylesheet present", "@media print" in page)
    ok("respects a dark theme", "prefers-color-scheme" in page)
    ok("page declares a language", bool(re.search(r"<html[^>]+lang=", page)))
    if a.rebuild:
        out = tempfile.mktemp(suffix=".html")
        subprocess.run(a.rebuild.format(md=str(MD), out=out), shell=True, capture_output=True)
        ok("page is current with the markdown", pathlib.Path(out).read_text() == page)
        os.unlink(out)
    else:
        # Without --rebuild the page cannot be regenerated and compared, but staleness
        # is still worth catching, because it does not only come from the markdown:
        # md2html INLINES an SVG figure, so regenerating that SVG leaves the published
        # page showing the old drawing while the markdown is untouched and every other
        # check passes. Falling back to modification times catches exactly that, and
        # says plainly that it is the weaker test.
        srcs = [MD] + [MD.parent / m for m in
                       re.findall(r'!\[[^\]]*\]\(([^)\s]+\.svg)\)', src)]
        srcs = [p for p in srcs if p.exists()]
        try:
            ht = pathlib.Path(a.html).stat().st_mtime
            stale = sorted({p.name for p in srcs if p.stat().st_mtime > ht + 1})
            ok("page is no older than its sources", not stale,
               f"newer than the page: {stale}" if stale
               else f"mtime only, {len(srcs)} source(s); pass --rebuild to compare content")
        except OSError:
            pass

# ── 7. fenced blocks presented as terminal sessions ──────────────────────────
BLOCKS = re.findall(r"```\n\$ ([^\n]+)\n((?:.*\n)*?)```", src)
if a.run_blocks and BLOCKS:
    # A quoted command is often a generator, not a read-only query: the octagonal
    # torus writeup quotes torus-geometry-diagram.js, which rewrites the very figure
    # the document displays. Auditing must not mutate the tree it audits, and the
    # damage is silent — the next run's "page is current" check would fail against an
    # HTML rebuilt from a figure this tool had replaced. So snapshot ROOT, run the
    # blocks against the real tree for fidelity, then put back anything they touched.
    #
    # Guarded on BLOCKS because the snapshot is not free. A writeup with no fenced
    # sessions was reading every byte under ROOT to protect against commands that do
    # not exist -- and then failing, because a 148MB video sat over the cap and got
    # reported as "may have modified". Nothing had run. Nothing could have.
    STASH_CAP = 8 * 1024 * 1024
    SNAP_SKIP = {".git", ".DS_Store"}

    def walk():
        for p in ROOT.rglob("*"):
            if p.is_file() and not (SNAP_SKIP & set(p.parts)):
                yield p

    def stamp(p):
        st = p.stat()
        return (st.st_size, st.st_mtime_ns)

    # Files over the cap are tracked by size and mtime instead of content. That cannot
    # restore one, but it can tell whether restoring is even called for -- and saying
    # "a block may have modified" about a file that provably did not change is a
    # failure the reader has no way to dismiss.
    before, big = {}, {}
    for p in walk():
        try:
            if p.stat().st_size <= STASH_CAP:
                before[p] = p.read_bytes()
            else:
                big[p] = stamp(p)
        except OSError:
            big[p] = None

    for cmd, body in BLOCKS:
        try:
            got = subprocess.run(cmd, shell=True, cwd=ROOT, capture_output=True,
                                 text=True, timeout=60).stdout
        except Exception as e:
            ok(f"`$ {cmd}` runs", False, str(e)); continue
        want = body.rstrip("\n").split("\n")
        have = [l for l in got.rstrip("\n").split("\n")][:len(want)]
        ok(f"quoted output of `$ {cmd[:44]}` is verbatim", want == have,
           "" if want == have else "block differs from a live run")

    put_back, unrestorable = [], []
    for p in walk():                                    # changed, or newly created
        rel = str(p.relative_to(ROOT))
        if p in big:
            was = big[p]
            try:
                if was is not None and stamp(p) != was:
                    unrestorable.append(rel)
            except OSError:
                unrestorable.append(rel)
            continue
        if p not in before:
            p.unlink(); put_back.append(rel + " (created)"); continue
        keep = before[p]
        if p.read_bytes() != keep:
            p.write_bytes(keep); put_back.append(rel)
    for p, keep in before.items():                      # or deleted outright
        if not p.exists():
            p.write_bytes(keep); put_back.append(str(p.relative_to(ROOT)) + " (deleted)")
    for p in big:                                       # walk() cannot see these
        if not p.exists():
            unrestorable.append(str(p.relative_to(ROOT)) + " (deleted)")
    if put_back:
        notes.append("  – blocks wrote to the tree; restored " + ", ".join(sorted(put_back)))
    if unrestorable:
        fails.append("  ✗ a block changed a file too large to restore: "
                     + ", ".join(sorted(unrestorable)))

# ── 8. external links ────────────────────────────────────────────────────────
if a.links:
    for u in sorted(set(re.findall(r'https://[^)\s"<>]+', src))):
        try:
            rq = urllib.request.Request(u, headers={"User-Agent": "Mozilla/5.0"})
            code = urllib.request.urlopen(rq, timeout=25).status
        except Exception as e:
            code = getattr(e, "code", str(e)[:30])
        ok(f"{u[:66]}", code == 200, f"HTTP {code}" if code != 200 else "")

for line in notes:
    print(line)
print()
for line in fails:
    print(line)
print(f"\n  {len(notes)} passed, {len(fails)} failed")
sys.exit(1 if fails else 0)
