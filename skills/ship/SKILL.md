---
name: ship
description: Commit, gate and push LaserMadeMusic work — stage by name, commit to main, run all-gates.sh in the background with output filtered to failures, and push only when the sole failure is the commits waiting to go up. Use when asked to commit and push, ship, publish or "send it up" in any repository under ~/LaserMadeMusic, or after finishing a change there that the user has asked to land.
---

# Ship LaserMadeMusic work

The standing rules for these repositories: **commit straight to `main`, push without
asking, stage by name, and gate before pushing.** This is that sequence, done the same
way every time, with the gate's output kept to the lines that matter.

## 1. Look before staging

```sh
git -C REPO status --short
git -C REPO log --oneline @{u}..HEAD     # commits already waiting, not yours
```

- **Stage by name.** Never `git add -A`, `git add .` or `git commit -a`: the author edits
  SVGs in Inkscape during a session, and a blanket add commits work in progress.
- Anything in `status` you did not change: leave it unstaged and say so.
- Commits already waiting that you did not make: they will go up with yours. Name them
  in your report.

## 2. Commit

One commit per coherent change, message in the repository's style (read
`git log --oneline -10`), ending with the attribution lines the session gives you.
Check any number you put in the message (line counts, file counts) before committing.

If the change touched a writeup, regenerate and audit its page first. One line from
the writeup's folder, quiet unless something fails:

```sh
~/LaserMadeMusic/GIT/lasermade-tools/rebuild-page.sh            # README.md -> index.html
~/LaserMadeMusic/GIT/lasermade-tools/rebuild-page.sh --md=WRITEUP.md --html=PAGE.html
```

Commit the regenerated `index.html` with the README.

## 3. Gate

Which repositories the gate covers is the `REPOS=` line near the top of
`lasermade-tools/all-gates.sh`; read it rather than trusting a list here.

**In the gate:** run it in the background, filtered, and do not poll. It takes about
ten minutes and the harness says when it ends:

```sh
cd ~/LaserMadeMusic/GIT/lasermade-tools && bash all-gates.sh 2>&1 | grep -E "FAIL|GATES"; echo done
```

- **Change nothing in any gated repository while it runs.** It checks every repo is
  clean, and an edit mid-run fails that or audits a half-written file. Prepare further
  edits in the scratchpad and copy them in afterwards.
- The unfiltered output is about forty lines; the filtered one is a handful. Do not run
  it unfiltered to "see progress".

**Not in the gate** (e.g. `~/LaserMadeMusic/octomino-snakes`): there is no gate to run.
Audit what changed instead: `doc-audit.py FILE` for markdown, the repository's own
check for anything else (its CLAUDE.md says which). A non-git folder has nothing to push.

## 4. Read the verdict, then push or stop

- **Only `every repo pushed -- FAIL N unpushed`**, and N equals the commits you mean to
  push (yours plus any already waiting, across all repos): that is the gate seeing your
  commits. Push.
- **Anything else fails:** do not push. Report the failing lines as printed. Fix only
  what your change caused; a failure that predates it is the user's call.

```sh
git -C REPO push -q origin main && git -C REPO status -sb | head -1   # expect main...origin/main
```

- **Push refused because the GitHub repository is archived:** stop and ask. With the
  user's go-ahead: `gh repo unarchive OWNER/REPO --yes`, push, then ask whether to
  re-archive (`gh repo archive OWNER/REPO --yes`). Don't leave it unarchived silently.

## 5. Report

One short block: what was committed (hash and subject), what was pushed, the gate
verdict, and anything left unpushed or unstaged and why.
