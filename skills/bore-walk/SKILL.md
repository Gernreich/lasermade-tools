---
name: bore-walk
description: Intake for a trumpet bore walk such as "N1 W3 U2 E3 N3 D3 W2 U3 N1" — split it with bore_split.py, lead with the elbow count, build the turnable viewer and publish it as an artifact link. Use whenever the user supplies, pastes or proposes a bore walk (a string of N/S/E/W/U/D terms with distances), asks what a walk costs, or asks for a walk's viewer.
---

# Bore walk intake

Every supplied walk gets the same treatment, agreed with the user: **split it, lead with
the elbow count, build the viewer, give the link.** Writing cut files or filing a design
happens only when asked.

The tools are in `~/LaserMadeMusic/GIT/trumpet-elbows-allowed/tools/`, the only live
trumpet repository. `bore_split.py` there is **the authority** on what a walk costs: over
this skill, over any CLAUDE.md, over a hand count.

## Vocabulary, before anything else

A walk is terms like `N3`: a direction (`U`/`D` ±Y, `N` −Z, `S` +Z, `E` +X, `W` −X,
Minecraft's axes) and a distance in blocks. Blocks = 1 + the sum of the distances. You
enter facing the first term and leave facing the last; there are no bare-letter ends.

"Bend" and "elbow" are **piece kinds with a price**, not descriptions:

- **bend**: a turn with a straight block either side, folded into an L. Free.
- **elbow**: a turn stranded as its own one-block piece. It costs flat-to-flat gluing,
  flattened plates and an unfilled corner void, the hardest part of the build.

If a word in the request could change what counts as a cost ("only bends", "tighter",
"no stranding"), and its meaning is not settled in this conversation, check
`tools/CLAUDE.md` or ask before optimising anything.

## 1. Split it

```sh
cd ~/LaserMadeMusic/GIT/trumpet-elbows-allowed/tools
python3 bore_split.py --no-write "<walk>"
```

It prints blocks, centreline length, pieces, and an assembly table whose `kind` column
says `straight`, `bend` or `elbow`. Exit 0 with elbows is normal here: this repository
cuts them. A refusal (exit 1) means the walk is genuinely broken, usually a
transcription error that runs it into itself. Suspect a wrong direction before a wrong
length, and quote the message.

## 2. Report, elbow count first

- **Elbows: N**, and which pieces (block ranges). Zero: say zero.
- Then blocks, centreline mm, piece count, and any `!` warnings it printed (e.g. two
  elbows meeting directly).
- **Elbows are allowed by default** (the user's rule since 2026-10-06). Report them as
  information, not as a defect: do not exclude, reject or "fix" the walk for having them.
- **Unless told otherwise for this request:** if the user says no elbows / bend-only,
  the target is zero, not "fewest". `--refuse-elbows` makes the tool refuse instead.
- If asked how to remove an elbow: whether a turn folds depends on each window of
  three consecutive terms (outer A, middle m, outer C). Same axis and direction (step):
  m ≥ 1. Same axis, opposite direction (hairpin): m ≥ 2. Different axes (coil): m ≥ 3.
  Name the term that would have to grow, then re-run the tool to confirm.

## 3. Build the viewer and publish it

A walk in a message is a request for the viewer as well, always ("do this anytime I
supply a walk"). The user reads walks off a Minecraft model, and a still picture can't
show where a spiral goes.

```sh
python3 viewer.py "<walk>" --out SCRATCH/<name>.html --title "<Title>"
```

- `SCRATCH` is the session scratchpad. Never write into the repository unless asked.
- If the walk already has a design folder, `bore_split.py --write DIR` writes the viewer
  as `DIR/<dirname>.html` with the cut files. That rewrites the folder, so only when asked.
- Publish the HTML with the Artifact tool and give the link. Keep the `<title>` the same
  across redeploys; to republish a renamed file, pass the old artifact URL as `url`.

## 4. Only when asked

Cut files, a design folder, a `regress.py` entry, a walk file in `tools/walks/`. Then
follow `tools/CLAUDE.md`: the Boxes venv for anything that gates
(`~/Software/boxes/venv/bin/python`), `--blocksize`/`--bore` as the design needs, and
the `ship` skill to land it.
