#!/usr/bin/env bash
# Puts everything the Flatpak build would download next to the sources, so
# that `flatpak-builder` can build Cantara offline. See
# `packaging/flatpak/app.cantara.Cantara.yml`.
#
#   packaging/scripts/prepare-flatpak-sources.sh
#
# Needs cargo, npm and the Dioxus CLI (`dx`, at the version Cargo.toml asks
# for) on PATH. Writes into `.flatpak/`, which is ignored by git.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

out=".flatpak"
mkdir -p "$out"

# The npm packages the build script would otherwise install (see
# `ensure_npm_packages` in build.rs). The browser tests' packages are not
# needed for a build.
npm ci --omit=dev --no-audit --no-fund

# Every crate, as the build will see it. The path is where flatpak-builder
# puts this module's sources.
rm -rf "$out/cargo-vendor"
cargo vendor --locked --versioned-dirs "$out/cargo-vendor" > /dev/null

# The lock files some crates ship. Cargo ignores them in a dependency — except
# that cantara-songlib's build script runs cbindgen, and cbindgen runs `cargo
# metadata` on the crate by itself, lock file included. Offline that asks for
# the exact versions the crate was published with, which are not the ones
# vendored here, and the build stops. Without the lock file it settles on the
# vendored versions. The lock files are listed in each crate's checksums, so
# they come out of those as well.
python3 - "$out/cargo-vendor" <<'EOF'
import json, pathlib, sys
for checksums in pathlib.Path(sys.argv[1]).glob("*/.cargo-checksum.json"):
    lock = checksums.parent / "Cargo.lock"
    if lock.exists():
        lock.unlink()
        data = json.loads(checksums.read_text())
        data["files"].pop("Cargo.lock", None)
        checksums.write_text(json.dumps(data))
EOF
cat > "$out/cargo-config.toml" <<'EOF'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "/run/build/cantara/.flatpak/cargo-vendor"
EOF

# The Dioxus CLI. It runs inside the SDK, whose glibc is newer than that of
# any machine the release workflow builds on, so the binary from here works
# there.
dx_bin="$(command -v dx)"
install -m755 "$dx_bin" "$out/dx"

echo "Flatpak sources prepared in $out/"
