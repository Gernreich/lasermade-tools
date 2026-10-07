#!/usr/bin/env python3
"""Convert a build-writeup markdown file to a self-contained styled HTML page."""
import html
import re
import sys
import pathlib
import shutil
import subprocess
import tempfile

# A MISSING ARGUMENT CAME BACK AS AN IndexError. This is the most-used tool in
# the repository -- every published page in nine repositories goes through it --
# and run bare it answered with a stack trace on sys.argv[1] rather than saying
# what it takes.
if len(sys.argv) < 3:
    sys.exit("usage: md2html.py SOURCE.md OUTPUT.html")
src = pathlib.Path(sys.argv[1])
dst = pathlib.Path(sys.argv[2])
if not src.exists():
    sys.exit(f"md2html: no such file: {src}")
lines = src.read_text().split("\n")

# The tab title is the document's own first h1, so one converter serves several
# writeups. Falls back to the filename for a document that has none. Strips ` and *
# but NOT _, which is a literal in names like knot_soundhole rather than emphasis.
title = next((re.sub(r"[`*]", "", ln[2:]).strip() for ln in lines if ln.startswith("# ")),
             src.stem)


# PANDOC DOES THE PARSING. Until 2026-10-06 this file was its own markdown parser,
# about 250 lines of regular expressions, and every construct it had not met yet
# came out wrong on the published page: raw HTML escaped into visible source, a
# blockquote kept its "> " markers, indented code was joined into prose, an image
# wrapped onto a second line published as literal ![...](...). Each was found
# after it went out. Pandoc implements the whole of GitHub-flavoured markdown, so
# the page now reads the way GitHub renders the README. What stays here is only
# what is ours: the two markers below, the filter, and the page shell.
if not shutil.which("pandoc"):
    sys.exit("md2html: needs pandoc (brew install pandoc)")

# The markers are applied to the text before pandoc sees it, and never inside a
# fenced code block, where they are being quoted rather than used.
#
# <!-- readme-only --> drops the paragraph that follows it. A repository whose
# page is generated from its own README ends up publishing that README's
# "Read the writeup" line on the writeup itself, linking to the page you are
# already reading. Marked rather than detected: the converter knows only its
# input and output paths, and guessing the site URL from the directory name is
# wrong the moment a directory and its repository differ - as test/ and
# bore-designs do. GitHub renders the comment as nothing, so the README is
# unaffected.
#
# <!-- page-only TEXT --> is the reverse: GitHub hides the whole comment, and
# here TEXT replaces the line and is read as ordinary markdown. It carries the
# page's link back to its README, which on the README would point at itself.
kept, i, fenced = [], 0, False
while i < len(lines):
    ln = lines[i]
    if ln.startswith("```"):
        fenced = not fenced
    elif not fenced and ln.strip() == "<!-- readme-only -->":
        i += 1
        while i < len(lines) and not lines[i].strip():   # any blank lines after it
            i += 1
        while i < len(lines) and lines[i].strip():       # then the paragraph itself
            i += 1
        continue
    elif not fenced and (m := re.match(r"^<!-- page-only (.+) -->$", ln.strip())):
        ln = m.group(1)
    kept.append(ln)
    i += 1

# The filter holds the three things pandoc would otherwise do differently.
FILTER = r'''
-- Runs with the source's own directory as its working directory, so the image
-- paths the markdown gives resolve exactly as they do beside it.

-- A sibling writeup ships as HTML beside this page. The markdown keeps its .md
-- link, which is what GitHub's own file view needs; the generated page points at
-- the generated page, or Pages would serve raw markdown instead. The fragment is
-- split off first, so "guide.md#section" becomes "guide.html#section".
function Link(el)
  local path, frag = el.target:match("^([^#]*)(.*)$")
  if path:match("%.md$") and not el.target:match("^%a[%w+.-]*:") then
    el.target = path:sub(1, -4) .. ".html" .. frag
  end
  return el
end

-- The table's box: a border, a card background, and on a narrow screen a
-- sideways scroll instead of a page wider than the phone.
function Table(t)
  return pandoc.Div({t}, pandoc.Attr("", {"tw"}))
end

local function esc(s)
  return (s:gsub("&", "&amp;"):gsub("<", "&lt;"):gsub(">", "&gt;")
           :gsub('"', "&quot;"):gsub("'", "&#x27;"))
end

-- An image alone in its paragraph is a figure, and a local SVG is inlined so the
-- drawing is part of the page. An inlined SVG is not an <img> and carries no alt,
-- so the alt text written in the markdown becomes its aria-label -- unless the
-- file has a label of its own, which ships with the drawing and wins. Without
-- that the description reached nothing, and in living-hinge the markdown and the
-- drawing had already drifted apart ("x across the width" against "x runs across
-- the width"). doc-audit's "every figure has a text alternative" found it.
function Para(p)
  if #p.content ~= 1 or p.content[1].t ~= "Image" then return nil end
  local img = p.content[1]
  local alt = pandoc.utils.stringify(img.caption)
  local f = img.src:lower():match("%.svg$") and io.open(img.src)
  if f then
    local svg = f:read("a"); f:close()
    svg = svg:gsub("<%?xml[^>]*%?>%s*", ""):gsub("<!DOCTYPE[^>]*>%s*", "")
    if alt ~= "" and not svg:match("^[^>]*>"):find("aria-label", 1, true) then
      svg = svg:gsub("<svg%f[%A]", '<svg aria-label="' .. esc(alt):gsub("%%", "%%%%") .. '"', 1)
    end
    svg = svg:gsub("^%s+", ""):gsub("%s+$", "")
    return pandoc.RawBlock("html", "<figure>" .. svg .. "</figure>")
  end
  return pandoc.RawBlock("html", '<figure><img src="' .. esc(img.src) ..
                         '" alt="' .. esc(alt) .. '"></figure>')
end
'''

with tempfile.TemporaryDirectory() as tmp:
    flt = pathlib.Path(tmp, "md2html.lua")
    flt.write_text(FILTER)
    srcfile = pathlib.Path(tmp, src.name)
    srcfile.write_text("\n".join(kept))
    run = subprocess.run(
        ["pandoc", "-f", "gfm", "-t", "html5", "--wrap=none",
         "--syntax-highlighting=none", "--lua-filter", str(flt), str(srcfile)],
        capture_output=True, text=True, cwd=src.parent.resolve())
    if run.returncode:
        sys.exit(f"md2html: pandoc failed:\n{run.stderr}")
body = run.stdout

# The contents list, from the headings pandoc wrote. Built here rather than with
# pandoc's --toc because its list nests each level under the one above, and a page
# with no h1 would then show every h2 at the top level, bold, as if it were one.
# Level 1 and 2 only, each marked with its own level, as before.
toc = [(int(m.group(1)), html.unescape(re.sub(r"<[^>]+>", "", m.group(3))), m.group(2))
       for m in re.finditer(r'<h([12]) id="([^"]+)"[^>]*>(.*?)</h\1>', body, re.S)]
nav = "".join(
    f'<a class="l{l}" href="#{s}">{html.escape(t)}</a>' for l, t, s in toc
)

CSS = """
:root{--bg:#fbfbfa;--fg:#1a1c1e;--mut:#5b6067;--line:#e2e4e7;--card:#fff;--accent:#1d4ed8;
--codebg:#f4f5f7;--thbg:#f0f1f3;--mark:#fff8c4}
@media (prefers-color-scheme:dark){:root{--bg:#15171a;--fg:#e6e8ea;--mut:#9aa1a9;--line:#2b2f34;
--card:#1b1e22;--accent:#7aa2f7;--codebg:#22262b;--thbg:#23272c;--mark:#4a411c}}
:root[data-theme=dark]{--bg:#15171a;--fg:#e6e8ea;--mut:#9aa1a9;--line:#2b2f34;--card:#1b1e22;
--accent:#7aa2f7;--codebg:#22262b;--thbg:#23272c;--mark:#4a411c}
:root[data-theme=light]{--bg:#fbfbfa;--fg:#1a1c1e;--mut:#5b6067;--line:#e2e4e7;--card:#fff;
--accent:#1d4ed8;--codebg:#f4f5f7;--thbg:#f0f1f3;--mark:#fff8c4}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);
font:16px/1.62 ui-sans-serif,system-ui,-apple-system,"Segoe UI",Helvetica,Arial,sans-serif;
-webkit-text-size-adjust:100%}
.wrap{display:grid;grid-template-columns:250px minmax(0,1fr);gap:44px;max-width:1180px;
margin:0 auto;padding:40px 28px 96px}
nav{position:sticky;top:28px;align-self:start;max-height:calc(100vh - 56px);overflow:auto;
border-right:1px solid var(--line);padding-right:16px;font-size:13.5px;line-height:1.45}
nav b{display:block;font-size:11px;letter-spacing:.09em;text-transform:uppercase;color:var(--mut);
margin:0 0 12px}
nav a{display:block;color:var(--mut);text-decoration:none;padding:3px 0;border-radius:4px}
nav a:hover{color:var(--accent)}
nav a.l1{color:var(--fg);font-weight:640;margin-top:11px}
nav a.l2{padding-left:12px}
main{min-width:0}
h1{font-size:1.62rem;line-height:1.25;letter-spacing:-.015em;margin:2.4em 0 .5em;
padding-bottom:.32em;border-bottom:2px solid var(--line)}
main>h1:first-child{margin-top:0;border-bottom:none;font-size:1.95rem}
h2{font-size:1.16rem;margin:2em 0 .5em;letter-spacing:-.01em}
h3{font-size:1.02rem;font-weight:650;margin:1.7em 0 .45em}
p{margin:.85em 0}
ul,ol{margin:.85em 0;padding-left:1.35em}
li{margin:.3em 0}
/* A list with blank lines between its items is "loose", and pandoc wraps each item
   in a <p> as GitHub does. These pages were set tight before pandoc, so they stay so. */
li>p{margin:0}
hr{border:0;border-top:1px solid var(--line);margin:2.6em 0}
figure{margin:1.6em 0}
/* A paragraph holding two or more images is a gallery. Left to inline layout they
   wrap at whatever fits, which put three bells on one row and the fourth alone
   underneath - directly above the caption, so it read as the caption for that one
   picture rather than for the set. Flex lets them share the row instead. */
p:has(img + img){display:flex;flex-wrap:wrap;gap:10px;align-items:flex-start;
justify-content:center}
p:has(img + img) img{flex:1 1 140px;min-width:0;width:auto;max-width:100%;height:auto;
border:1px solid var(--line);border-radius:10px}
figure svg,figure img{display:block;width:100%;max-width:100%;height:auto;
border:1px solid var(--line);border-radius:10px}
a{color:var(--accent)}
code{font:13px/1.5 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;
background:var(--codebg);padding:.1em .32em;border-radius:4px;
border:1px solid color-mix(in srgb,var(--line) 70%,transparent)}
pre{background:var(--codebg);border:1px solid var(--line);border-radius:9px;
padding:14px 16px;overflow-x:auto;margin:1.1em 0}
pre code{background:none;border:0;padding:0;font-size:12.8px;line-height:1.62;white-space:pre}
blockquote{margin:1.15em 0;padding:.7em 1.1em;border-left:4px solid var(--accent);
background:var(--codebg);border-radius:0 7px 7px 0}
blockquote p{margin:0}
.tw{overflow-x:auto;margin:1.15em 0;border:1px solid var(--line);border-radius:9px;
background:var(--card)}
table{border-collapse:collapse;width:100%;font-size:14px}
th,td{padding:8px 13px;text-align:left;border-bottom:1px solid var(--line);vertical-align:top}
/* honour align="center" on gallery cells: a presentation attribute loses to any
   CSS rule, so the line above would silently left-align every thumbnail caption */
td[align=center],th[align=center]{text-align:center}
th{background:var(--thbg);font-weight:630;white-space:nowrap;font-size:13px;
letter-spacing:.01em}
tbody tr:last-child td{border-bottom:0}
tbody tr:hover{background:color-mix(in srgb,var(--thbg) 45%,transparent)}
td code,th code{white-space:nowrap}
/* A walk inside a link is one token: breaking it mid-walk put "N" at the end of one
   line and "N5 U" at the start of the next, which reads as two things. */
a code{white-space:nowrap}
.tog{position:fixed;top:14px;right:14px;z-index:9;background:var(--card);color:var(--mut);
border:1px solid var(--line);border-radius:7px;padding:6px 11px;font-size:12.5px;cursor:pointer}
.tog:hover{color:var(--accent)}
/* Below 900px the contents list moves to the top of the page, and the fixed theme
   button sat on top of its first lines. There it scrolls away with the page instead,
   in a strip of padding kept free for it; fixed, it would ride over the text as the
   page scrolls, since a narrow screen has no margin for it to sit in. */
@media (max-width:900px){.wrap{grid-template-columns:1fr;gap:20px;padding:62px 18px 72px}
.tog{position:absolute}
nav{position:static;max-height:none;border-right:0;border-bottom:1px solid var(--line);
padding:0 0 16px;columns:2;column-gap:20px}}
@media print{nav,.tog{display:none}.wrap{display:block;max-width:none;padding:0}
pre,.tw{break-inside:avoid}h1{break-after:avoid}}
"""

JS = """
var r=document.documentElement,b=document.querySelector('.tog');
function set(t){r.setAttribute('data-theme',t);try{localStorage.setItem('ctg-theme',t)}catch(e){}
b.textContent=t==='dark'?'\\u2600 light':'\\u263e dark';}
var saved=null;try{saved=localStorage.getItem('ctg-theme')}catch(e){}
set(saved||(matchMedia('(prefers-color-scheme:dark)').matches?'dark':'light'));
b.addEventListener('click',function(){set(r.getAttribute('data-theme')==='dark'?'light':'dark')});
"""

doc = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{html.escape(title)}</title>
<style>{CSS}</style>
</head>
<body>
<button class="tog" type="button">theme</button>
<div class="wrap">
<nav><b>Contents</b>{nav}</nav>
<main>
{body.rstrip()}
</main>
</div>
<script>{JS}</script>
</body>
</html>
"""
dst.write_text(doc)
print(f"wrote {dst}  ({len(doc)} bytes, {len(toc)} toc entries)")
