#!/usr/bin/env bash
# Installs the APK on a running emulator, opens Cantara and checks that it is
# still there afterwards, with its web view on screen.
#
#   packaging/scripts/smoke-test-android.sh <cantara.apk> <output dir>
#
# The release workflow runs this before the APK is attached to a release: an
# APK that verifies can still be one that closes the moment it is opened, which
# is what the earlier builds did on every phone (see build-android-apk.sh).
# Leaves logcat, the UI tree and a screenshot in the output dir.
set -euo pipefail

apk="${1:?usage: smoke-test-android.sh <cantara.apk> <output dir>}"
out="${2:?usage: smoke-test-android.sh <cantara.apk> <output dir>}"
package="app.cantara.Cantara"
mkdir -p "$out"

adb wait-for-device
adb install -r "$apk"
adb logcat -c
adb shell am start -W -n "$package/dev.dioxus.main.MainActivity"

# Long enough for the native library to load and the first page to render on a
# software-rendered emulator.
sleep 45

adb logcat -d > "$out/logcat.txt" || true
adb exec-out screencap -p > "$out/screenshot.png" || true
adb shell uiautomator dump /sdcard/ui.xml > /dev/null 2>&1 || true
adb pull /sdcard/ui.xml "$out/ui.xml" > /dev/null 2>&1 || true

failed=0
pid="$(adb shell pidof "$package" | tr -d '\r' || true)"
if [ -z "$pid" ]; then
    echo "::error::Cantara is not running 45 seconds after it was started."
    failed=1
else
    echo "Cantara is running (pid $pid)."
fi

if grep -E "FATAL EXCEPTION|Fatal signal|SIGABRT|UnsatisfiedLinkError" "$out/logcat.txt" | grep -i -E "cantara|dioxus" ; then
    echo "::error::The log shows Cantara crashing (see logcat.txt)."
    failed=1
fi

if [ -f "$out/ui.xml" ] && grep -q 'class="android.webkit.WebView"' "$out/ui.xml"; then
    echo "The web view is on screen."
else
    echo "::error::No web view on screen: the app started, but its window did not come up."
    failed=1
fi

if [ "$failed" -ne 0 ]; then
    echo "--- the end of logcat ---"
    tail -n 200 "$out/logcat.txt" || true
fi
exit "$failed"
