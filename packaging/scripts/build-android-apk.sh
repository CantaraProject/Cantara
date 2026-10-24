#!/usr/bin/env bash
# Builds the Android APK: installable, signed, and for the processors phones
# actually have.
#
#   packaging/scripts/build-android-apk.sh <output.apk>
#
# Needs the Android SDK (ANDROID_HOME), the NDK (ANDROID_NDK_HOME), Java 17,
# the Rust targets aarch64-linux-android and x86_64-linux-android, and `dx`.
# Optional, for the release key (see "Signing" below):
# ANDROID_KEYSTORE_BASE64, ANDROID_KEYSTORE_PASSWORD, ANDROID_KEY_ALIAS,
# ANDROID_KEY_PASSWORD. ANDROID_BUILD_TOOLS_VERSION picks the build tools
# (default 34.0.0).
#
# What `dx build --platform android` alone gets wrong, and this fixes:
#
# * The processor. Without `--target`, dx builds for the machine it runs on —
#   on a CI runner that is x86_64, a library no phone can load, and the app
#   closes the moment it is opened. This builds arm64-v8a, which every phone of
#   the last ten years has, and x86_64 for emulators and Chromebooks, and puts
#   both into the one APK; Android picks the right one when it installs it.
# * The signature. Gradle's `assembleRelease` without a signing configuration
#   writes `app-release-unsigned.apk`, which Android refuses to install.
# * The version. dx writes `versionCode = 1` into every build, and Android
#   orders updates by that number, not by the version name.
# * The icon, which would be Dioxus' own.
set -euo pipefail

output="$(realpath -m "${1:?usage: build-android-apk.sh <output.apk>}")"
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

build_tools="${ANDROID_HOME:?ANDROID_HOME is not set}/build-tools/${ANDROID_BUILD_TOOLS_VERSION:-34.0.0}"
project="target/dx/cantara/release/android/app"
jnilibs="$project/app/src/main/jniLibs"

# --- The libraries, one build per processor ----------------------------------
#
# Each build writes the Gradle project anew but replaces only its own ABI's
# directory under jniLibs/. The libraries of the earlier builds are set aside
# and put back anyway, so that the result does not depend on that detail.
stash="$(mktemp -d)"
trap 'rm -rf "$stash"' EXIT
for pair in aarch64-linux-android:arm64-v8a x86_64-linux-android:x86_64; do
    triple="${pair%%:*}"
    abi="${pair##*:}"
    echo "::group::dx build for $abi"
    dx build --platform android --release --target "$triple" --locked
    echo "::endgroup::"
    if [ ! -f "$jnilibs/$abi/libdioxusmain.so" ] && ! ls "$jnilibs/$abi/"*.so > /dev/null 2>&1; then
        echo "dx build for $triple left no library in $jnilibs/$abi" >&2
        exit 1
    fi
    cp -r "$jnilibs/$abi" "$stash/"
done
for abi_dir in "$stash"/*; do
    rm -rf "${jnilibs:?}/$(basename "$abi_dir")"
    cp -r "$abi_dir" "$jnilibs/"
done
echo "Native libraries in the APK:"
find "$jnilibs" -name '*.so' -exec ls -la {} \;

# --- The version code ---------------------------------------------------------
#
# major.minor.patch as MMMmmmppp: 3.1.2 → 3001002. Grows with every release,
# which is all Android asks of it.
version="$(perl -ne 'if (/^version = "([^"]*)"/) { print $1; exit }' Cargo.toml)"
core="${version%%[-+]*}"
IFS=. read -r major minor patch <<< "$core"
version_code=$(( major * 1000000 + minor * 1000 + patch ))
if [ "$version_code" -lt 1 ]; then
    version_code=1
fi
echo "versionName $version, versionCode $version_code"
perl -pi -e "s/versionCode = \\d+/versionCode = $version_code/" "$project/app/build.gradle.kts"
grep -q "versionCode = $version_code" "$project/app/build.gradle.kts"

# --- Shared storage on Android 10 ---------------------------------------------
#
# Android 10 hides every file in shared storage that is not a picture, video
# or song from an app — except an app that says it wants the old behaviour.
# Android 11 and newer ignore the attribute and go by "access to all files"
# instead (see src/logic/storage_access.rs). dx has no setting for it, so it
# goes into the manifest dx wrote.
manifest="$project/app/src/main/AndroidManifest.xml"
if ! grep -q 'requestLegacyExternalStorage' "$manifest"; then
    perl -0pi -e 's/<application\b/<application android:requestLegacyExternalStorage="true"/' "$manifest"
fi
grep -q 'android:requestLegacyExternalStorage="true"' "$manifest"
grep -q 'android.permission.MANAGE_EXTERNAL_STORAGE' "$manifest"
# The theme that lets the page follow the system's dark mode
# (`[android.application]` in Dioxus.toml); without it the page is always light.
grep -q 'Theme.AppCompat.DayNight' "$manifest"

# --- The icon -----------------------------------------------------------------
#
# Dioxus' launcher icon is an adaptive icon (mipmap-anydpi-v26) over WebP
# fallbacks. Both go: two files of the same resource name and different
# extensions do not build.
res="$project/app/src/main/res"
rm -rf "$res"/mipmap-*
cp -r packaging/android/res/. "$res/"

# --- The APK --------------------------------------------------------------------
echo "::group::gradle assembleRelease"
(
    cd "$project"
    chmod +x gradlew
    ./gradlew assembleRelease --no-daemon
)
echo "::endgroup::"

apk_dir="$project/app/build/outputs/apk/release"
# Whatever Gradle called it — `app-release-unsigned.apk` without a signing
# config, `app-release.apk` with one.
unsigned="$(ls "$apk_dir"/*.apk | head -n 1)"

# --- Signing ------------------------------------------------------------------
#
# The key comes from the repository secrets when they are set. Without them a
# key is made here so that the pipeline still produces something installable —
# but it is a different key on every run, and Android will not install an
# update signed by a different key than the version already on the device. Set
# the secrets before handing builds to anyone.
keystore="$stash/cantara.jks"
if [ -n "${ANDROID_KEYSTORE_BASE64:-}" ]; then
    echo "${ANDROID_KEYSTORE_BASE64}" | base64 --decode > "$keystore"
    key_alias="${ANDROID_KEY_ALIAS:?ANDROID_KEY_ALIAS must be set with ANDROID_KEYSTORE_BASE64}"
    store_password="${ANDROID_KEYSTORE_PASSWORD:?ANDROID_KEYSTORE_PASSWORD must be set with ANDROID_KEYSTORE_BASE64}"
    key_password="${ANDROID_KEY_PASSWORD:-$store_password}"
    echo "Signing with the key from the repository secrets."
else
    generated="cantara-ci-$(date +%s)-$RANDOM"
    if [ -n "${GITHUB_ACTIONS:-}" ]; then
        echo "::add-mask::${generated}"
    fi
    keytool -genkeypair \
        -keystore "$keystore" \
        -alias cantara \
        -keyalg RSA -keysize 4096 -validity 10000 \
        -storepass "$generated" -keypass "$generated" \
        -dname "CN=Cantara CI, O=CantaraProject, C=DE" > /dev/null
    key_alias="cantara"
    store_password="$generated"
    key_password="$generated"
    echo "::warning::No ANDROID_KEYSTORE_BASE64 secret is set, so this APK is signed with a throwaway key. It installs, but it cannot be installed *over* a build signed with any other key."
fi

# Alignment has to happen before signing: zipalign rewrites the archive, which
# would invalidate a signature already in it.
"$build_tools/zipalign" -p -f 4 "$unsigned" "$stash/aligned.apk"
mkdir -p "$(dirname "$output")"
"$build_tools/apksigner" sign \
    --ks "$keystore" \
    --ks-key-alias "$key_alias" \
    --ks-pass "pass:$store_password" \
    --key-pass "pass:$key_password" \
    --out "$output" \
    "$stash/aligned.apk"

# --- Checks -----------------------------------------------------------------------
#
# An APK that does not verify here is one that a phone will refuse, and one
# without an arm64 library is one that no phone can run.
"$build_tools/apksigner" verify --verbose --print-certs "$output"
contents="$(unzip -l "$output")"
for abi in arm64-v8a x86_64; do
    if ! grep -q "lib/$abi/.*\.so" <<< "$contents"; then
        echo "The APK has no native library for $abi" >&2
        exit 1
    fi
done
echo "Built $output"
