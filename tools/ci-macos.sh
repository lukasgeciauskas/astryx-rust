#!/usr/bin/env bash
# macOS CI checks that need the screen: screenshots of the gallery and of the
# GPUI fork's astryx_* examples (each window captured alone with screencapture),
# and the fork's Metal renderer tests. Screenshots and logs go to
# target/screenshots, which the workflow uploads.
#
# usage: tools/ci-macos.sh gallery|fork
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
shots="$root/target/screenshots"
mkdir -p "$shots"
status=0

window_id_tool="$root/target/mac-window-id"
if [ ! -x "$window_id_tool" ]; then
    swiftc -O "$here/mac-window-id.swift" -o "$window_id_tool" || exit 1
fi

# capture <name> <seconds to wait> <command...>: runs the command, waits, then
# captures its window and quits it.
capture() {
    local name="$1" wait="$2"
    shift 2
    echo "PROGRESS: capturing $name"
    "$@" >"$shots/$name.log" 2>&1 &
    local pid=$!
    local id=""
    for _ in $(seq 1 "$wait"); do
        sleep 1
        kill -0 "$pid" 2>/dev/null || break
    done
    if kill -0 "$pid" 2>/dev/null; then
        id="$("$window_id_tool" "$pid")"
    fi
    if [ -n "$id" ]; then
        screencapture -x -o -l "$id" "$shots/$name.png" || status=1
    else
        echo "::error::$name has no window (it exited or never opened one); see $name.log"
        tail -n 40 "$shots/$name.log"
        status=1
    fi
    kill "$pid" 2>/dev/null
    wait "$pid" 2>/dev/null
}

gallery() {
    local exe="$root/target/debug/rustryx-gallery"
    # Text in the system font, Kbd's Apple symbols, the controls, the fields
    # (spelling marks), dates, overlays, and the offscreen layers (Effects, Motion).
    for page in Tokens Text Content Buttons Inputs Dates Overlays Effects Motion; do
        capture "gallery-$page-light" 20 env GALLERY_PAGE="$page" GALLERY_MODE=light "$exe"
    done
    for page in Tokens Effects; do
        capture "gallery-$page-dark" 20 env GALLERY_PAGE="$page" GALLERY_MODE=dark "$exe"
    done
    # Lower on the Effects page: gradients and the edge-fade masks (offscreen layers).
    for scroll in 900 1500; do
        capture "gallery-Effects-$scroll" 20 \
            env GALLERY_PAGE=Effects GALLERY_MODE=light GALLERY_SCROLL="$scroll" "$exe"
    done
    # The default theme's stacks start with -apple-system: the system font.
    capture "gallery-Text-default-theme" 20 \
        env GALLERY_THEME=default GALLERY_PAGE=Text GALLERY_MODE=light "$exe"
}

fork() {
    local manifest src
    manifest="$(cd "$root" && cargo metadata --format-version 1 |
        jq -r '.packages[] | select(.name == "gpui") | .manifest_path')"
    src="$(dirname "$(dirname "$(dirname "$manifest")")")"
    echo "GPUI fork checkout: $src"
    export CARGO_TARGET_DIR="$root/target/fork"
    # The Metal renderer's tests (offscreen layers among them) and the native
    # pickers' value conversions.
    (cd "$src" && cargo test -p gpui_apple --lib) || status=1
    (cd "$src" && cargo test -p gpui_macos --lib native_picker) || status=1
    local examples="$CARGO_TARGET_DIR/debug/examples"
    for example in astryx_layers astryx_transforms astryx_native_picker astryx_inline \
        astryx_layout_glide astryx_scrolling astryx_sticky; do
        if (cd "$src" && cargo build -p gpui --example "$example"); then
            capture "$example" 15 "$examples/$example"
        else
            status=1
        fi
    done
    # Each native picker open, then the layers drawn directly (the fallback).
    for kind in date time datetime; do
        capture "astryx_native_picker-$kind" 15 \
            env ASTRYX_NATIVE_PICKER_OPEN="$kind" "$examples/astryx_native_picker"
    done
    capture "astryx_layers-fallback" 15 \
        env GPUI_DISABLE_OFFSCREEN_LAYERS=1 "$examples/astryx_layers"
}

case "${1:-}" in
gallery) gallery ;;
fork) fork ;;
*)
    echo "usage: tools/ci-macos.sh gallery|fork" >&2
    exit 2
    ;;
esac
exit "$status"
