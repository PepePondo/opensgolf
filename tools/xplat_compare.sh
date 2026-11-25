#!/bin/sh
# Compares the simulation fingerprints of the same scripted game played on different platforms (the cross-platform
# determinism workflow, .github/workflows/determinism-xplat.yml). Each FILE is one platform's SG_FINGERPRINT file written
# with SG_FINGERPRINT_PARTS=1 ("TICK TOTAL section=hash ..."); the first is the reference. For each other file it prints
# the first game tick where the game's state parts (every section but club_unsaved and inputs, which are what the
# window hands the simulation) differ from the reference and which sections differ there, and separately the first
# tick where the totals (state and inputs) differ. Exit status 1 when any state differs.
#
# usage: tools/xplat_compare.sh REF.fp OTHER.fp...
set -u
ref=$1
shift
fail=0
echo "reference: $ref ($(grep -c . "$ref") game ticks)"
for f in "$@"; do
    out=$(awk '
        NR == FNR { a[$1] = $0; next }
        ($1 in a) {
            shared++
            n = split(a[$1], x, " "); split($0, y, " ")
            if (total == "" && x[2] != y[2]) total = $1
            if (state != "") next
            parts = ""
            for (i = 3; i <= n; i++) {
                if (x[i] == y[i]) continue
                k = x[i]; sub(/=.*/, "", k)
                if (k == "club_unsaved" || k == "inputs") { inp = inp == "" ? $1 : inp; continue }
                parts = parts " " k
            }
            if (parts != "") { state = $1; where = parts }
        }
        END {
            printf "%d shared ticks; ", shared
            if (state != "") printf "STATE DIFFERS from tick %s:%s", state, where
            else printf "state identical"
            if (total != "") printf "; totals differ from tick %s", total
            if (inp != "") printf " (window inputs differ from tick %s)", inp
            print ""
            exit (state != "" || shared == 0) ? 1 : 0
        }' "$ref" "$f")
    code=$?
    [ $code -ne 0 ] && fail=1
    echo "$(basename "$f"): $out"
done
exit $fail
