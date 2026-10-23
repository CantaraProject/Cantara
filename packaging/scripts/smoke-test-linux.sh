#!/usr/bin/env bash
# Starts Cantara on a virtual display and checks that it is still running
# afterwards, with WebKitGTK's web process started for its window.
#
#   packaging/scripts/smoke-test-linux.sh <output dir> <command> [args…]
#
# The command is whatever starts the package under test: `cantara` after
# installing the .deb, the AppImage, `flatpak run app.cantara.Cantara`. Needs
# Xvfb; leaves the program's output (and a screenshot, if ImageMagick's
# `import` is there) in the output dir.
set -euo pipefail

out="${1:?usage: smoke-test-linux.sh <output dir> <command> [args…]}"
shift
mkdir -p "$out"
out="$(realpath "$out")"

display=":$(( 90 + RANDOM % 9 ))"
Xvfb "$display" -screen 0 1600x1000x24 -nolisten tcp > "$out/xvfb.log" 2>&1 &
xvfb_pid=$!
# shellcheck disable=SC2317  # called by the trap
cleanup() {
    # The whole process group: WebKitGTK's helpers outlive their parent.
    if [ -n "$app_pid" ]; then
        kill -- "-$app_pid" 2> /dev/null || kill "$app_pid" 2> /dev/null || true
    fi
    kill "$xvfb_pid" 2> /dev/null || true
}
app_pid=""
trap cleanup EXIT
sleep 2

# A fresh home, so that the test sees a first start and leaves nothing behind.
HOME="$(mktemp -d)"
export HOME
export XDG_RUNTIME_DIR="$HOME/run"
mkdir "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
export DISPLAY="$display"
# No GPU on a CI machine; WebKitGTK's compositing would otherwise fail on the
# virtual display before it draws anything.
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export LIBGL_ALWAYS_SOFTWARE=1

# In a process group of its own (setsid), so that it can be ended as a whole.
if command -v dbus-run-session > /dev/null; then
    setsid dbus-run-session -- "$@" > "$out/output.log" 2>&1 &
else
    setsid "$@" > "$out/output.log" 2>&1 &
fi
app_pid=$!

sleep 25

if command -v import > /dev/null; then
    import -display "$display" -window root "$out/screenshot.png" 2> /dev/null || true
fi

failed=0
if ! kill -0 "$app_pid" 2> /dev/null; then
    wait "$app_pid" && status=0 || status=$?
    echo "::error::Cantara exited (status $status) within 25 seconds of being started."
    failed=1
elif ! pgrep -f WebKitWebProcess > /dev/null; then
    echo "::error::Cantara is running, but WebKitGTK never started a web process: its window has nothing in it."
    failed=1
else
    echo "Cantara is running, and so is its web process."
fi

if [ "$failed" -ne 0 ]; then
    echo "--- output ---"
    cat "$out/output.log" || true
fi
exit "$failed"
