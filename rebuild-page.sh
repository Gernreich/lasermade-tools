#!/bin/bash
# rebuild-page.sh -- regenerate a writeup's page and audit the pair, in one line.
#
#   rebuild-page.sh [DIR] [--md=README.md] [--html=index.html] [--links]
#
# Runs md2html.py and then doc-audit.py --rebuild from DIR (default: here), so
# the paths come out right wherever it is called from. That is the reason it
# exists: the same two commands were written out by hand in eight CLAUDE.md
# files, each with its own relative path to this repository, and one of them
# pointed at a README that was not there.
#
# Prints only the audit's failed lines and its tally, so a clean run is one
# line. Fail-closed like all-gates.sh: it exits 0 only when it SEES "0 failed",
# so a crash that prints nothing is a failure, not a pass.
set -u
G="$(cd "$(dirname "$0")" && pwd)"
DIR=. MD=README.md HTML=index.html LINKS=
for a in "$@"; do
  case "$a" in
    --md=*)   MD="${a#--md=}" ;;
    --html=*) HTML="${a#--html=}" ;;
    --links)  LINKS=--links ;;
    -h|--help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "rebuild-page.sh: unknown option $a" >&2; exit 2 ;;
    *)  DIR="$a" ;;
  esac
done
cd "$DIR" || { echo "rebuild-page.sh: no directory $DIR" >&2; exit 2; }
[ -f "$MD" ] || { echo "rebuild-page.sh: no $MD in $(pwd)" >&2; exit 2; }

python3 "$G/md2html.py" "$MD" "$HTML" >/dev/null || { echo "FAIL md2html.py $MD" >&2; exit 1; }
out=$(python3 "$G/doc-audit.py" "$MD" --html "$HTML" \
        --rebuild "python3 $G/md2html.py {md} {out}" $LINKS 2>&1)
echo "$out" | grep -E '✗' | cut -c1-200
tally=$(echo "$out" | grep -E 'passed,' | tail -1)
echo "${tally:-FAIL doc-audit printed no tally}  $(pwd)/$MD"
[[ "$tally" == *" 0 failed"* ]]
