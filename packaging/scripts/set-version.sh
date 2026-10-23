#!/usr/bin/env bash
# Stamps the version of a release into the files the packages are made from.
#
#   packaging/scripts/set-version.sh <tag> [--core]
#
# The tag is what was released on GitHub (`v3.0.0`, `3.0.0-beta.1`, …). Every
# installer takes its version from `Cargo.toml`, so that is where it goes; a
# release whose tag and `Cargo.toml` disagree would otherwise ship packages
# that call themselves something else than the release they are attached to.
#
# `--core` drops a pre-release suffix (`3.0.0-beta.1` → `3.0.0`). The Windows
# installer needs it: NSIS writes the version into the executable's VERSIONINFO,
# which only takes numbers.
#
# Also adds the release to the AppStream metadata, which software centres show
# as the version history, and prints `version=…` and `version_core=…` for
# `$GITHUB_OUTPUT`.
#
# Plain POSIX tools and perl only: it runs on the Linux, macOS and Windows
# (Git Bash) runners alike, and `sed -i` is not the same on any two of them.
set -euo pipefail

tag="${1:?usage: set-version.sh <tag> [--core]}"
version="${tag#v}"

if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'; then
    echo "::error::'$tag' is not a semantic version (expected e.g. v3.0.0 or v3.0.0-beta.1)" >&2
    exit 1
fi

version_core="${version%%[-+]*}"
stamped="$version"
if [ "${2:-}" = "--core" ]; then
    stamped="$version_core"
fi

root="$(cd "$(dirname "$0")/../.." && pwd)"

# The first `version = ` in Cargo.toml is the one in [package].
STAMPED="$stamped" perl -0pi -e 's/^version = "[^"]*"/version = "$ENV{STAMPED}"/m' "$root/Cargo.toml"
# And the lock file with it, or every `--locked` build stops at the mismatch.
STAMPED="$stamped" perl -0pi -e 's/(\nname = "cantara"\r?\nversion = )"[^"]*"/$1"$ENV{STAMPED}"/' "$root/Cargo.lock"

metainfo="$root/packaging/linux/app.cantara.Cantara.metainfo.xml"
if ! grep -q "<release version=\"$version\"" "$metainfo"; then
    TAG="$tag" VERSION="$version" DATE="$(date -u +%Y-%m-%d)" perl -0pi -e \
        's|(<releases>\n)|$1        <release version="$ENV{VERSION}" date="$ENV{DATE}">\n            <url type="details">https://github.com/CantaraProject/Cantara/releases/tag/$ENV{TAG}</url>\n        </release>\n|' \
        "$metainfo"
fi

echo "version=$version"
echo "version_core=$version_core"
