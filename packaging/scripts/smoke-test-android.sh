#!/usr/bin/env bash
# Installs the APK on a running emulator, opens Cantara and checks that it is
# still there afterwards, with its web view on screen.
#
#   packaging/scripts/smoke-test-android.sh <cantara.apk> <output dir>
#
# The release workflow runs this before the APK is attached to a release: an
# APK that verifies can still be one that closes the moment it is opened, which
# is what the earlier builds did on every phone (see build-android-apk.sh).
#
# Leaves in the output dir: the whole logcat, Cantara's own part of it, the UI
# tree and a screenshot. On failure it prints Cantara's part, and what the
# system said about the app — not the end of the system log, which is other
# apps' noise.
set -euo pipefail

apk="${1:?usage: smoke-test-android.sh <cantara.apk> <output dir>}"
out="${2:?usage: smoke-test-android.sh <cantara.apk> <output dir>}"
package="app.cantara.Cantara"
mkdir -p "$out"

# The app's process id, or nothing. Two ways, because a missing pid is what
# fails the test and should not hinge on one tool: `pidof` matches the process
# name, `ps` lists it.
app_pid() {
    local pid
    pid="$(adb shell pidof "$package" 2> /dev/null | tr -d '\r' || true)"
    if [ -z "$pid" ]; then
        pid="$(adb shell ps -A -o PID,NAME 2> /dev/null | tr -d '\r' | awk -v name="$package" '$2 == name { print $1; exit }' || true)"
    fi
    echo "$pid"
}

adb wait-for-device
adb install -r "$apk"
adb logcat -c
adb shell am start -W -n "$package/dev.dioxus.main.MainActivity"

# Watched for 45 seconds — long enough for the native library to load and the
# first page to render on a software-rendered emulator — so that a failure says
# *when* the app went away, not only that it did.
first_pid=""
died_after=""
for second in $(seq 5 5 45); do
    sleep 5
    pid="$(app_pid)"
    if [ -n "$pid" ]; then
        if [ -z "$first_pid" ]; then
            first_pid="$pid"
            echo "Cantara is running (pid $pid) after ${second}s."
        elif [ "$pid" != "$first_pid" ]; then
            echo "Cantara was restarted: pid $first_pid, now $pid (after ${second}s)."
            died_after="$second"
            break
        fi
    else
        echo "Cantara is not running after ${second}s."
        died_after="$second"
        break
    fi
done

# The evidence, gathered after the watch so that it covers it entirely.
adb exec-out screencap -p > "$out/screenshot.png" || true
adb shell uiautomator dump /sdcard/ui.xml > /dev/null 2>&1 || true
adb pull /sdcard/ui.xml "$out/ui.xml" > /dev/null 2>&1 || true
adb logcat -d > "$out/logcat.txt" || true
if [ -n "$first_pid" ]; then
    adb logcat -d --pid="$first_pid" > "$out/logcat-cantara.txt" || true
fi
# What the system said about the app: crashes (the package name is on the line
# after "FATAL EXCEPTION", hence the context), native aborts, ANRs, kills.
grep -n -A 30 -E "FATAL EXCEPTION|Fatal signal|ANR in $package|UnsatisfiedLinkError" "$out/logcat.txt" \
    | grep -B 2 -A 28 -F "$package" > "$out/logcat-crash.txt" || true
grep -E "$package" "$out/logcat.txt" \
    | grep -i -E "died|kill|crash|anr|force.?stop|force.?finish|lowmemory|removing|exited" > "$out/logcat-lifecycle.txt" || true

failed=0
if [ -z "$first_pid" ]; then
    echo "::error::Cantara never showed up as a process."
    failed=1
elif [ -n "$died_after" ]; then
    echo "::error::Cantara did not stay up: gone after ${died_after}s (see logcat-crash.txt and logcat-lifecycle.txt)."
    failed=1
else
    echo "Cantara is still running after 45s."
fi

if [ -s "$out/logcat-crash.txt" ]; then
    echo "::error::The log shows Cantara crashing (see logcat-crash.txt)."
    failed=1
fi

# A web view of *this* app: the UI tree holds every window on screen.
if [ -f "$out/ui.xml" ] && grep -o '<node [^>]*>' "$out/ui.xml" \
        | grep 'class="android.webkit.WebView"' | grep -q "package=\"$package\""; then
    echo "Cantara's web view is on screen."
else
    echo "::error::No web view of Cantara on screen: the app started, but its window did not come up."
    failed=1
fi

if [ "$failed" -ne 0 ]; then
    for part in logcat-crash logcat-lifecycle logcat-cantara; do
        if [ -s "$out/$part.txt" ]; then
            echo "--- $part.txt ---"
            tail -n 150 "$out/$part.txt"
        fi
    done
fi
exit "$failed"
