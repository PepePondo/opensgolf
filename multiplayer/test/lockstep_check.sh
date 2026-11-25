#!/bin/sh
# Lockstep check: plays the same scripted game under different screens (window size, view turn, zoom, how many ticks
# run between drawn frames) and compares the simulation's fingerprints. With SG_LOCKSTEP=1 every variant must match the
# first; without it they are expected to drift (that shows the check can see a difference).
#
# usage: multiplayer/test/lockstep_check.sh [OUT_DIR] [SECONDS]   (needs a build with --features multiplayer, xvfb-run)
set -u
out=${1:-/tmp/lockstep}
time=${2:-400}
exe=${EXE:-target/release/opensgolf}
edit="t:16,30,0;t:32,24,1;p:20,29,2,0,1;p:24,28,2,0,1;p:28,26,2,0,1;o;h:2,24,27;h:3,22,26;y:36,20;j:0"
mkdir -p "$out"
play() { # name lockstep step size rot zoom
    mkdir -p "$out/$1.save"
    SG_LOCKSTEP=$2 SG_MP_STEP=$3 SG_FINGERPRINT_PARTS=1 SG_FINGERPRINT="$out/$1.fp" OPENSGOLF_SAVE_DIR="$out/$1.save" \
        LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a "$exe" --game "$out/no-game" --mute --size "$4" --rot "$5" --zoom "$6" \
        --property 0 --clock 4242 --edit "$edit" --time "$time" --png "$out/$1.png" > "$out/$1.log" 2>&1
}
status=0
for ls in 0 1; do
    play "ls$ls-a" $ls 0.25 800x600 0 1
    play "ls$ls-b" $ls 0.6 1280x720 90 2
    play "ls$ls-c" $ls 1.3 640x480 270 0.5
    echo "SG_LOCKSTEP=$ls:"
    if sh tools/xplat_compare.sh "$out/ls$ls-a.fp" "$out/ls$ls-b.fp" "$out/ls$ls-c.fp"; then
        echo "  all the same"
    elif [ "$ls" = 1 ]; then
        status=1
    fi
done
exit $status
