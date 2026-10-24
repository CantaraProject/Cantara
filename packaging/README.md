# Packaging

Everything Cantara is distributed as is built by
[`.github/workflows/release.yml`](../.github/workflows/release.yml) when a
release is published on GitHub, and attached to that release:

| File | For |
|---|---|
| `Cantara-<version>-windows-x64-setup.exe` | Windows 10/11. Installs the WebView2 runtime through Microsoft's bootstrapper if it is missing (needs internet then). |
| `Cantara-<version>-windows-x64-setup-with-webview2.exe` | Windows without internet access: the complete WebView2 runtime is inside. |
| `cantara_<version>_amd64.deb` | Debian 12+, Ubuntu 22.04+ and derivatives. A pre-release carries Debian's version spelling (`3.0.0~beta.1`), so that apt updates from it to the release. |
| `Cantara-<version>-x86_64.AppImage` | Any other x86_64 Linux with glibc 2.35 or newer. Brings its own WebKitGTK and the GStreamer plugins video slides need. |
| `Cantara-<version>-x86_64.flatpak` | Any Linux with Flatpak: `flatpak install Cantara-<version>-x86_64.flatpak` (the GNOME runtime comes from Flathub). |
| `Cantara-<version>-macos-arm64.dmg` | Macs with Apple silicon, macOS 11+. |
| `Cantara-<version>-macos-x86_64.dmg` | Macs with an Intel processor, macOS 11+. |
| `Cantara-<version>-android.apk` | Android 7+ (arm64 phones and tablets, x86_64 emulators and Chromebooks). |
| `SHA256SUMS.txt` | `sha256sum -c SHA256SUMS.txt` checks all of the above. It always covers every package attached to the release, also after a rebuild of only some of them. |

The version is taken from the release's tag (`v3.0.0`, `v3.1.0-beta.1`) and
stamped into `Cargo.toml` before the build; the tag is what decides, not what
`Cargo.toml` says at the time. Each package is started once before it is
attached (see the workflow for how), so a package that builds but does not
start fails the run instead of reaching anybody.

To build the packages for an existing release again, run the workflow by hand
(*Actions → Release packages → Run workflow*) with the release's tag. Assets of
the same name are replaced.

## Secrets

All optional; without them the packages are built and work, with the
limitations given.

| Secret | Without it |
|---|---|
| `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | Every APK is signed with a new throwaway key, so an installed Cantara cannot be updated in place — it has to be uninstalled first. **Set these before the first release that people install.** |
| `APPLE_CERTIFICATE` (Developer ID Application certificate as base64 `.p12`), `APPLE_CERTIFICATE_PASSWORD` | The macOS app is signed ad hoc. macOS asks before the first start: right-click the app → *Open*, or run `xattr -dr com.apple.quarantine /Applications/Cantara.app`. |
| `APPLE_ID`, `APPLE_PASSWORD` (app-specific password), `APPLE_TEAM_ID` | A signed app is not notarized. |

An Android key is made once with

```bash
keytool -genkeypair -v -keystore cantara-release.jks -alias cantara \
  -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 cantara-release.jks   # → ANDROID_KEYSTORE_BASE64
```

Keep the `.jks` file safe: an app signed with a different key cannot update
the installed one.

## What is here

- `linux/` – desktop entry, AppStream metadata and icons, shared by the
  Debian package, the AppImage and the Flatpak. `cantara.desktop.hbs` is the
  desktop entry as dx's Debian bundler takes it; `AppRun` is what the
  AppImage starts.
- `flatpak/app.cantara.Cantara.yml` – the Flatpak manifest, modelled on
  [Cantara 2's](https://github.com/flathub/app.cantara.Cantara). It builds
  offline from sources that `scripts/prepare-flatpak-sources.sh` puts next to
  the checkout first.
- `android/res/` – the launcher icon in Android's densities.
- `scripts/` – what the workflow runs; every script says at the top what it
  does and why. They run locally as well, e.g.

  ```bash
  dx build --release --platform linux
  packaging/scripts/build-appimage.sh target/dx/cantara/release/linux/app Cantara.AppImage
  ```

The settings `dx bundle` takes (identifier, icons, descriptions, Debian
section, WebView2 mode, NSIS install mode, minimum macOS version) are in
[`Dioxus.toml`](../Dioxus.toml) under `[bundle]`.
