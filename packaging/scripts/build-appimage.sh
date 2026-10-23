#!/usr/bin/env bash
# Builds the AppImage from what `dx build --release --platform linux` produced.
#
#   packaging/scripts/build-appimage.sh <dx app dir> <output file>
#
# `dx bundle --package-types appimage` exists, but what it makes only starts on
# a machine that has the very same WebKitGTK installed: WebKitGTK does not draw
# a page in the process that asks for it, it starts `WebKitWebProcess` and
# `WebKitNetworkProcess` for that, from a directory compiled into the library
# (`/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1`). Bundle the library without its
# helpers, and the window stays empty — or the program aborts — wherever that
# directory is missing or holds a different version.
#
# So this does what Tauri's bundler does for the same problem:
#
#   1. the helper processes and the injected bundle go into the AppDir at the
#      path they have on the build machine;
#   2. linuxdeploy's GTK plugin bundles GTK with its loaders and schemas and
#      rewrites `/usr` in the bundled libwebkit2gtk to `././`, an equally long
#      relative path;
#   3. the classic AppImageKit `AppRun` changes into `$APPDIR/usr` before it
#      starts Cantara, so that relative path lands on the bundled helpers.
#
# Run it on the oldest distribution the AppImage should start on (the release
# workflow uses Ubuntu 22.04): everything is taken from the build machine, and
# glibc is the one library that is never bundled.
set -euo pipefail

app_dir="$(realpath "${1:?usage: build-appimage.sh <dx app dir> <output file>}")"
output="$(realpath -m "${2:?usage: build-appimage.sh <dx app dir> <output file>}")"
root="$(cd "$(dirname "$0")/../.." && pwd)"
arch="x86_64"

# The tools, at fixed versions and checked against fixed hashes: what ends up
# in the AppImage should not depend on what somebody uploaded last night.
# These are the builds Tauri's bundler uses.
linuxdeploy_url="https://github.com/tauri-apps/binary-releases/releases/download/linuxdeploy-07333c6/linuxdeploy-x86_64.AppImage"
linuxdeploy_sha="36a2d7e274d12e1050d0e9ecfe11d339ed54720b2bec464c286d53f8b07f5c62"
apprun_url="https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-x86_64"
apprun_sha="f30140a43a0a59e46db21bdefdf749b9e9f2c6946e92afabbacf98b8ae73fb4f"
gtk_plugin_url="https://raw.githubusercontent.com/tauri-apps/tauri/c464ef928c35d7eedd64bcd5df16b62b3393b7fa/crates/tauri-bundler/src/bundle/linux/appimage/linuxdeploy-plugin-gtk.sh"
gtk_plugin_sha="ef6b9a980417243bc62e0241b51dc49876032afd1bab9b4762389f961b406d9b"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
tools="$work/tools"
mkdir -p "$tools"

fetch() {
    local url="$1" sha="$2" dest="$3"
    curl --fail --location --silent --show-error --retry 3 --output "$dest" "$url"
    echo "$sha  $dest" | sha256sum --check --quiet
    chmod +x "$dest"
}
fetch "$linuxdeploy_url" "$linuxdeploy_sha" "$tools/linuxdeploy"
fetch "$apprun_url" "$apprun_sha" "$tools/AppRun"
# linuxdeploy finds its plugins on PATH, by this name.
fetch "$gtk_plugin_url" "$gtk_plugin_sha" "$tools/linuxdeploy-plugin-gtk.sh"

appdir="$work/Cantara.AppDir"

# The layout Dioxus looks for its assets in: `<prefix>/bin/<exe>` next to
# `<prefix>/lib/<product name>/assets` (see `get_asset_root` in
# dioxus-asset-resolver).
install -Dm755 "$app_dir/cantara" "$appdir/usr/bin/cantara"
mkdir -p "$appdir/usr/lib/Cantara"
cp -r "$app_dir/assets" "$appdir/usr/lib/Cantara/assets"

install -Dm644 "$root/packaging/linux/app.cantara.Cantara.desktop" \
    "$appdir/usr/share/applications/app.cantara.Cantara.desktop"
install -Dm644 "$root/packaging/linux/app.cantara.Cantara.metainfo.xml" \
    "$appdir/usr/share/metainfo/app.cantara.Cantara.appdata.xml"
for size in 64 128 256 512; do
    install -Dm644 "$root/packaging/linux/icons/$size.png" \
        "$appdir/usr/share/icons/hicolor/${size}x${size}/apps/app.cantara.Cantara.png"
done

# Step 1: WebKitGTK's helper processes, at the path they have here.
webkit_dir=""
for candidate in /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1 /usr/lib64/webkit2gtk-4.1 /usr/lib/webkit2gtk-4.1 /usr/libexec/webkit2gtk-4.1; do
    if [ -x "$candidate/WebKitWebProcess" ]; then
        webkit_dir="$candidate"
        break
    fi
done
if [ -z "$webkit_dir" ]; then
    echo "WebKitGTK 4.1's helper processes were not found. Is libwebkit2gtk-4.1-0 installed?" >&2
    exit 1
fi
for file in WebKitWebProcess WebKitNetworkProcess injected-bundle/libwebkit2gtkinjectedbundle.so; do
    install -Dm755 "$webkit_dir/$file" "$appdir$webkit_dir/$file"
done

# Step 3: the AppRun that changes directory. linuxdeploy keeps an AppRun that
# is already there.
install -m755 "$tools/AppRun" "$appdir/AppRun"

# appimagetool would validate the AppStream file itself, online — and fail the
# build whenever the project's homepage is unreachable. The same check without
# the network, here, catches what is actually wrong with the file.
if command -v appstreamcli > /dev/null; then
    appstreamcli validate --no-net "$appdir/usr/share/metainfo/app.cantara.Cantara.appdata.xml"
fi

# Step 2. `--executable` for the helpers as well, so that what they link
# against is bundled, too.
(
    cd "$work"
    PATH="$tools:$PATH" \
    ARCH="$arch" \
    OUTPUT="$output" \
    APPIMAGE_EXTRACT_AND_RUN=1 \
    NO_STRIP=1 \
    NO_APPSTREAM=1 \
    "$tools/linuxdeploy" \
        --appimage-extract-and-run \
        --appdir "$appdir" \
        --executable "$appdir/usr/bin/cantara" \
        --executable "$appdir$webkit_dir/WebKitWebProcess" \
        --executable "$appdir$webkit_dir/WebKitNetworkProcess" \
        --desktop-file "$appdir/usr/share/applications/app.cantara.Cantara.desktop" \
        --icon-file "$appdir/usr/share/icons/hicolor/256x256/apps/app.cantara.Cantara.png" \
        --plugin gtk \
        --output appimage
)

if [ ! -f "$output" ]; then
    echo "linuxdeploy did not write $output" >&2
    exit 1
fi
chmod +x "$output"
echo "Built $output"
