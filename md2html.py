#!/usr/bin/env python3
"""Convert a build-writeup markdown file to a self-contained styled HTML page."""
import html
import re
import sys
import pathlib

src = pathlib.Path(sys.argv[1])
dst = pathlib.Path(sys.argv[2])
lines = src.read_text().split("\n")

# The tab title is the document's own first h1, so one converter serves several
# writeups. Falls back to the filename for a document that has none. Strips ` and *
# but NOT _, which is a literal in names like Octagonal_Torus rather than emphasis.
title = next((re.sub(r"[`*]", "", ln[2:]).strip() for ln in lines if ln.startswith("# ")),
             src.stem)


def inline(t):
    t = html.escape(t, quote=False)
    stash = []

    def keep(m):
        stash.append(m.group(1))
        return f"\x00{len(stash)-1}\x00"

    t = re.sub(r"`([^`]+)`", keep, t)
    t = re.sub(r"&lt;(https?://[^&\s]+)&gt;", r'<a href="\1">\1</a>', t)
    # any link target, not just http(s) — relative paths point at files beside this page.
    # (?<!!) so image syntax ![alt](src) is left for the block-level handler.
    def link(m):
        text, href = m.group(1), m.group(2)
        # A sibling writeup ships as HTML beside this page. The markdown keeps its .md
        # link, which is what GitHub's own file view needs; the generated page points
        # at the generated page, or Pages would serve raw markdown instead.
        # Split the fragment off first: "guide.md#section" must rewrite to
        # "guide.html#section", not stay pointing at raw markdown because the
        # string does not end in .md.
        path, sep, frag = href.partition("#")
        if path.endswith(".md") and not re.match(r"[a-zA-Z][a-zA-Z0-9+.-]*:", href):
            href = path[:-3] + ".html" + sep + frag
        return f'<a href="{href}">{text}</a>'

    t = re.sub(r"(?<!!)\[([^\]]+)\]\(([^)\s]+)\)", link, t)
    t = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", t)
    t = re.sub(r"(?<!\*)\*([^*\n]+?)\*(?!\*)", r"<em>\1</em>", t)
    t = re.sub(r"\x00(\d+)\x00", lambda m: f"<code>{stash[int(m.group(1))]}</code>", t)
    return t


out, toc = [], []
HTML_BLOCK = (r"(?:p|div|figure|figcaption|details|summary|section|article|aside"
              r"|blockquote|table|thead|tbody|tr|td|th|ul|ol|li|dl|dt|dd|h[1-6]"
              r"|pre|hr|iframe|video|picture|center)")

i, n = 0, len(lines)
while i < n:
    ln = lines[i]

    if ln.startswith("```"):
        block = []
        i += 1
        while i < n and not lines[i].startswith("```"):
            block.append(html.escape(lines[i], quote=False))
            i += 1
        i += 1
        out.append("<pre><code>" + "\n".join(block) + "</code></pre>")
        continue

    # <!-- readme-only --> drops the paragraph that follows it. A repository whose
    # page is generated from its own README ends up publishing that README's
    # "Read the writeup" line on the writeup itself, linking to the page you are
    # already reading. Marked rather than detected: the converter knows only its
    # input and output paths, and guessing the site URL from the directory name is
    # wrong the moment a directory and its repository differ - as test/ and
    # bore-designs do. GitHub renders the comment as nothing, so the README is
    # unaffected.
    if ln.strip() == "<!-- readme-only -->":
        while i < n and not lines[i].strip():          # any blank lines after it
            i += 1
        while i < n and lines[i].strip():              # then the paragraph itself
            i += 1
        continue

    # Raw HTML block, passed through verbatim (CommonMark type 7: a block-level
    # tag at column 0, running to the next blank line). Without this the whole
    # block falls through to the paragraph branch and gets html.escape()d, so a
    # gallery of <img> tags renders on the page as its own source code. GitHub's
    # markdown passes raw HTML through, which is why that reads fine on the repo
    # front page and only breaks on the generated site.
    if re.match(rf"^</?{HTML_BLOCK}[\s/>]", ln, re.I):
        blk = []
        while i < n and lines[i].strip():
            blk.append(lines[i])
            i += 1
        out.append("\n".join(blk))
        continue

    # Blockquote. Without this the "> " lines fall through to the paragraph
    # branch, which joins them and keeps every marker, so a callout renders as
    # "> text > more text" -- and a link split across two quoted lines swallows
    # the marker into its own link text.
    if ln.startswith(">"):
        quoted = []
        while i < n and lines[i].startswith(">"):
            quoted.append(re.sub(r"^>\s?", "", lines[i]))
            i += 1
        out.append("<blockquote><p>" + inline(" ".join(x.strip() for x in quoted)) + "</p></blockquote>")
        continue

    if re.match(r"^---+\s*$", ln):
        out.append("<hr>")
        i += 1
        continue

    m = re.match(r"^(#{1,4})\s+(.*)$", ln)
    if m:
        lvl, txt = len(m.group(1)), m.group(2)
        # GitHub's slug rule, so one anchor works both here and in the .md on GitHub:
        # downcase, drop anything that is not a letter/number/space/hyphen, spaces -> hyphens
        plain_txt = re.sub(r"[`*_]", "", txt)
        slug = re.sub(r"\s", "-", re.sub(r"[^a-z0-9 \-]", "", plain_txt.lower()))
        if lvl <= 2:
            plain = txt.replace("`", "").replace("**", "").replace("*", "")
            toc.append((lvl, plain, slug))
        out.append(f'<h{lvl} id="{slug}">{inline(txt)}</h{lvl}>')
        i += 1
        continue

    # table
    if ln.lstrip().startswith("|") and i + 1 < n and re.match(r"^\s*\|[\s:|-]+\|\s*$", lines[i + 1]):
        def cells(row):
            return [c.strip() for c in row.strip().strip("|").split("|")]
        head = cells(ln)
        i += 2
        body = []
        while i < n and lines[i].lstrip().startswith("|"):
            body.append(cells(lines[i]))
            i += 1
        t = ["<div class='tw'><table><thead><tr>"]
        t += [f"<th>{inline(c)}</th>" for c in head]
        t.append("</tr></thead><tbody>")
        for r in body:
            t.append("<tr>" + "".join(f"<td>{inline(c)}</td>" for c in r) + "</tr>")
        t.append("</tbody></table></div>")
        out.append("".join(t))
        continue

    mi = re.match(r"^!\[([^\]]*)\]\(([^)\s]+)\)\s*$", ln)
    if mi:
        alt, srcpath = mi.group(1), mi.group(2)
        cand = src.parent / srcpath
        if srcpath.lower().endswith(".svg") and cand.exists():
            svg = cand.read_text()
            svg = re.sub(r"<\?xml[^>]*\?>\s*", "", svg)
            svg = re.sub(r"<!DOCTYPE[^>]*>\s*", "", svg)
            out.append("<figure>" + svg.strip() + "</figure>")
        else:
            out.append(f'<figure><img src="{html.escape(srcpath)}" alt="{html.escape(alt)}"></figure>')
        i += 1
        continue

    if re.match(r"^\s*[-*]\s+", ln):
        items = []
        while i < n and re.match(r"^\s*[-*]\s+", lines[i]):
            item = re.sub(r"^\s*[-*]\s+", "", lines[i])
            i += 1
            while i < n and re.match(r"^\s{2,}\S", lines[i]) and not re.match(r"^\s*[-*]\s+", lines[i]):
                item += " " + lines[i].strip()
                i += 1
            items.append(item)
        out.append("<ul>" + "".join(f"<li>{inline(x)}</li>" for x in items) + "</ul>")
        continue

    if re.match(r"^\s*\d+\.\s+", ln):
        items = []
        while i < n and re.match(r"^\s*\d+\.\s+", lines[i]):
            item = re.sub(r"^\s*\d+\.\s+", "", lines[i])
            i += 1
            while i < n and re.match(r"^\s{3,}\S", lines[i]) and not re.match(r"^\s*\d+\.\s+", lines[i]):
                item += " " + lines[i].strip()
                i += 1
            items.append(item)
        out.append("<ol>" + "".join(f"<li>{inline(x)}</li>" for x in items) + "</ol>")
        continue

    if ln.strip() == "":
        i += 1
        continue

    para = [ln]
    i += 1
    while i < n and lines[i].strip() and not re.match(r"^(#{1,4}\s|```|---+\s*$|\s*[-*]\s|\s*\d+\.\s)", lines[i]) \
            and not lines[i].lstrip().startswith("|"):
        para.append(lines[i])
        i += 1
    out.append("<p>" + inline(" ".join(x.strip() for x in para)) + "</p>")

nav = "".join(
    f'<a class="l{l}" href="#{s}">{html.escape(t)}</a>' for l, t, s in toc if l <= 2
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
.tog{position:fixed;top:14px;right:14px;z-index:9;background:var(--card);color:var(--mut);
border:1px solid var(--line);border-radius:7px;padding:6px 11px;font-size:12.5px;cursor:pointer}
.tog:hover{color:var(--accent)}
@media (max-width:900px){.wrap{grid-template-columns:1fr;gap:20px;padding:26px 18px 72px}
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
{chr(10).join(out)}
</main>
</div>
<script>{JS}</script>
</body>
</html>
"""
dst.write_text(doc)
print(f"wrote {dst}  ({len(doc)} bytes, {len(toc)} toc entries)")
