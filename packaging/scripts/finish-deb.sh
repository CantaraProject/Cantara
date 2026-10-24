#!/usr/bin/env bash
# Gives the Debian package from `dx bundle` the version Debian understands.
#
#   packaging/scripts/finish-deb.sh <.deb from dx> <output .deb> <version>
#
# dx writes the Cargo version into the control file unchanged. For a release
# that is right; for a pre-release it is not: Debian reads `3.0.0-beta.1` as
# upstream version 3.0.0, Debian revision `beta.1`, which sorts *after* the
# final 3.0.0 — so apt would treat the update from the beta to the release as
# a downgrade and keep the beta. Debian's spelling of a pre-release is a tilde,
# `3.0.0~beta.1`, which sorts before 3.0.0.
#
# The file name keeps the release's own version; only the control file changes.
set -euo pipefail

input="${1:?usage: finish-deb.sh <.deb from dx> <output .deb> <version>}"
output="${2:?usage: finish-deb.sh <.deb from dx> <output .deb> <version>}"
version="${3:?usage: finish-deb.sh <.deb from dx> <output .deb> <version>}"

# Only the first hyphen starts the pre-release; build metadata (`+…`) is valid
# Debian as it is.
deb_version="${version/-/\~}"

mkdir -p "$(dirname "$output")"
if [ "$deb_version" = "$version" ]; then
    cp "$input" "$output"
else
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    dpkg-deb --raw-extract "$input" "$work/pkg"
    DEB_VERSION="$deb_version" perl -pi -e 's/^Version: .*$/Version: $ENV{DEB_VERSION}/' "$work/pkg/DEBIAN/control"
    dpkg-deb --root-owner-group --build "$work/pkg" "$output" > /dev/null
fi

actual="$(dpkg-deb --field "$output" Version)"
if [ "$actual" != "$deb_version" ]; then
    echo "The package says version '$actual', expected '$deb_version'" >&2
    exit 1
fi
echo "Built $output (Debian version $deb_version)"
