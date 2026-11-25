#!/bin/sh
# Builds the OpenSGolf Android app (an .apk) with cargo-quad-apk. Run it inside the notfl3/cargo-apk docker image, which has
# the Android SDK, the NDK and cargo-quad-apk:
#
#   docker run --rm -v "$(pwd)":/root/src -w /root/src notfl3/cargo-apk tools/android_build.sh [release|debug] [OUT_DIR]
#
# Out: OUT_DIR/opensgolf-<profile>.apk (default OUT_DIR: target/android), for arm64, 32-bit arm and x86_64 phones, signed
# with a debug key (cargo-quad-apk makes one), which is all Android needs to sideload it.
#
# cargo-quad-apk reads the manifests with the Cargo of Rust 1.61 (it is built on the cargo crate 0.62), which knows neither
# workspace inheritance (`version.workspace = true`), version 4 lock files nor crates of the 2024 edition. So the build
# runs on a copy of the source whose manifests spell the inherited fields out, with a lock file of its own that picks
# dependency versions made for Rust 1.84 or older (Cargo's rust-version aware resolver, from a current toolchain the
# script installs with rustup; that toolchain's rustc also compiles the game). The tree itself stays as it is.
set -e
cd "$(dirname "$0")/.."
src="$(pwd)"
profile="${1:-release}"
out="${2:-$src/target/android}"
work="${ANDROID_BUILD_DIR:-/tmp/opensgolf-android}"
targets="aarch64-linux-android armv7-linux-androideabi x86_64-linux-android"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
edition=$(sed -n 's/^edition = "\(.*\)"/\1/p' Cargo.toml | head -1)
license=$(sed -n 's/^license = "\(.*\)"/\1/p' Cargo.toml | head -1)

# a current Rust (the image's is older than some of the game's dependencies need); ANDROID_TARGETS builds fewer targets,
# SKIP_RUSTUP=1 uses a toolchain that is already there (a quick local build)
targets="${ANDROID_TARGETS:-$targets}"
if [ -z "$SKIP_RUSTUP" ]; then
    rustup toolchain install stable --profile minimal $(for t in $targets; do printf ' --target %s' "$t"; done)
    rustup default stable
fi

mkdir -p "$work"
# the source, without build output, the old C++ port or the web page (target/ is kept between runs)
tar cf - --exclude=./target --exclude=./.git --exclude=./legacy --exclude=./web --exclude=./build \
    --exclude=./game --exclude=./.claude . | (cd "$work" && tar xf -)
cd "$work"
for m in crates/*/Cargo.toml; do
    sed -i -e "s/^version.workspace = true/version = \"$version\"\nrust-version = \"1.84\"/" \
        -e "s/^edition.workspace = true/edition = \"$edition\"/" -e "s/^license.workspace = true/license = \"$license\"/" "$m"
done
if [ -n "$ANDROID_TARGETS" ]; then
    list=$(for t in $targets; do printf '"%s", ' "$t"; done)
    sed -i "s/^build_targets = .*/build_targets = [${list%, }]/" crates/simgolf/Cargo.toml
fi
# a store's build (OPENSGOLF_NO_SELF_UPDATE=1, the same rule as build.rs: set and neither empty nor "0") has no update
# check, Update now or play-time count, and asks for no permissions: INTERNET and REQUEST_INSTALL_PACKAGES come out
case "${OPENSGOLF_NO_SELF_UPDATE:-0}" in
    0) ;;
    *) sed -i -e '/^\[\[package.metadata.android.permission\]\]$/{N;/\nname = "android.permission.[A-Z_]*"$/d}' \
           crates/simgolf/Cargo.toml
       if grep -q '^\[\[package.metadata.android.permission\]\]$' crates/simgolf/Cargo.toml; then
           echo "error: a store build must ask for no permissions, crates/simgolf/Cargo.toml still has one" >&2
           exit 1
       fi ;;
esac
# the desktop's controller library (gamepad.rs) is not built for Android, and its uuid wants a newer js-sys than the old
# Cargo below can have: left out of the manifest here
sed -i -e '/^gilrs = /d' -e 's/^gamepad = \["dep:gilrs"\]/gamepad = []/' crates/simgolf/Cargo.toml
# The lock file for this build is kept in the repository (tools/android/Cargo.lock), so every build of a tag, a store's
# too, uses the same dependency versions. ANDROID_RELOCK=1 works the versions out again (for Rust 1.84, with the pins
# below) and writes them back there: run it after changing a dependency, then commit the file.
lock="$src/tools/android/Cargo.lock"
deps() { awk '/^name = /{n=$3} /^version = /{v=$3} /^source = /{print n, v, $3}' "$1"; }
if [ -f "$lock" ] && [ -z "$ANDROID_RELOCK" ]; then
    cp "$lock" Cargo.lock
    # only the game's own crates' versions may move (a version bump); a missing or changed dependency fails here
    CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update --workspace
    sed -i 's/^version = 4$/version = 3/' Cargo.lock
    # the crates.io packages (name, version, source) before and after must be the same
    if [ "$(deps Cargo.lock)" != "$(deps "$lock")" ]; then
        echo "error: tools/android/Cargo.lock no longer fits the manifests; run with ANDROID_RELOCK=1 and commit it" >&2
        exit 1
    fi
else
    rm -f Cargo.lock
    CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo generate-lockfile
    # the wasm-bindgen family (cpal's browser side, not built here) as of 2024: the newer ones declare features the old Cargo
    # cannot resolve (js-sys's `dep:futures-util`)
    # rhai's ahash and its const-random moved to getrandom 0.4 in November 2026 (ahash 0.8.13, const-random 0.1.19,
    # const-random-macro 0.1.17); getrandom 0.4 is a 2024 edition crate and wants a newer js-sys than the pin below
    cargo update -p ahash --precise 0.8.12
    cargo update -p const-random --precise 0.1.18
    cargo update -p const-random-macro --precise 0.1.16
    cargo update -p web-sys --precise 0.3.70
    cargo update -p wasm-bindgen-futures --precise 0.4.43
    cargo update -p js-sys --precise 0.3.70
    cargo update -p wasm-bindgen --precise 0.2.93
    # a version 3 lock file says the same for crates.io dependencies
    sed -i 's/^version = 4$/version = 3/' Cargo.lock
    mkdir -p "$(dirname "$lock")"
    cp Cargo.lock "$lock"
fi

# Sound goes through cpal's oboe output, whose C++ (oboe-sys) is compiled with the NDK. A failed build fails: it used to fall
# back to a build without the default features, which shipped a silent app with no update check for weeks without a word
# (a 32-bit overflow in autoupdate.rs broke the armeabi-v7a build; issue #191).
rel=""
if [ "$profile" = release ]; then rel="--release"; fi
# multiplayer (crates/simgolf/src/mp) in the GitHub APK; a store build (no permissions, so no internet) leaves it out
feat="--features multiplayer"
case "${OPENSGOLF_NO_SELF_UPDATE:-0}" in 0) ;; *) feat="" ;; esac
cargo quad-apk build $rel -p opensgolf $feat
# and the build itself must not have moved any of them
if [ "$(deps Cargo.lock)" != "$(deps "$lock")" ]; then
    echo "error: the build changed dependency versions in Cargo.lock (tools/android/Cargo.lock should fix them)" >&2
    exit 1
fi
# a store's build must not ask for any permission: checked on the manifest cargo-quad-apk wrote for the APK
case "${OPENSGOLF_NO_SELF_UPDATE:-0}" in
    0) ;;
    *) manifest=$(find "target/android-artifacts/$profile" -name AndroidManifest.xml | head -1)
       if [ -z "$manifest" ]; then
           echo "error: no AndroidManifest.xml under target/android-artifacts/$profile to check" >&2
           exit 1
       fi
       if grep -q 'uses-permission' "$manifest"; then
           echo "error: a store build must ask for no permissions, $manifest has:" >&2
           grep 'uses-permission' "$manifest" >&2
           exit 1
       fi ;;
esac
mkdir -p "$out"
apk=$(find "target/android-artifacts/$profile/apk" -name '*.apk' | head -1)
# the library the activity loads must be the one packed: cargo-quad-apk names the first after the package name's last part,
# the second after the crate (vendor/miniquad/README-OPENSGOLF.md); a mismatch closes the app at start on every phone
activity=$(find "target/android-artifacts/$profile" -name MainActivity.java | head -1)
lib=$(sed -n 's/.*System.loadLibrary("\([^"]*\)").*/\1/p' "${activity:-/dev/null}" | head -1)
if command -v unzip >/dev/null; then files=$(unzip -Z1 "$apk"); else files=$(jar tf "$apk"); fi
for t in $targets; do
    case "$t" in aarch64*) abi=arm64-v8a ;; armv7*) abi=armeabi-v7a ;; x86_64*) abi=x86_64 ;; *) abi=x86 ;; esac
    if ! echo "$files" | grep -qx "lib/$abi/lib$lib.so"; then
        echo "error: the app loads lib$lib.so, which is not in the APK for $abi:" >&2
        echo "$files" | grep "^lib/" >&2
        exit 1
    fi
done
cp "$apk" "$out/opensgolf-$profile.apk"
echo "built $out/opensgolf-$profile.apk"
