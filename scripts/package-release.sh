#!/usr/bin/env bash
# Packs one platform's release files into dist/release/ with a .sha256 file.
#
#   ./scripts/package-release.sh macos   v0.1.0-beta.1   # .dmg (run bundle-macos.sh first)
#   ./scripts/package-release.sh windows v0.1.0-beta.1   # .zip
#   ./scripts/package-release.sh linux   v0.1.0-beta.1   # .tar.gz
set -euo pipefail
cd "$(dirname "$0")/.."

platform=${1:?usage: package-release.sh macos|windows|linux vX.Y.Z[-pre]}
tag=${2:?missing version tag}
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "bad tag: $tag" >&2; exit 1; }
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ "v$version" = "$tag" ] || { echo "tag $tag does not match Cargo.toml version $version" >&2; exit 1; }

name="LazyArranger-$tag"
out="$PWD/dist/release"
mkdir -p "$out"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

notes() {
  cat <<TXT
Lazy Arranger Midi DAW $tag (pre-release)

$1

Open the app, then use Settings > Check for updates to see newer builds.
Licence: Apache-2.0 (see LICENSE). Bundled fonts: SIL OFL (see Inter-LICENSE.txt).
TXT
}

case "$platform" in
  macos)
    app="dist/Lazy Arranger Midi DAW.app"
    test -x "$app/Contents/MacOS/lazy-arranger-desktop"
    ditto "$app" "$stage/Lazy Arranger Midi DAW.app"
    ln -s /Applications "$stage/Applications"
    notes "Drag Lazy Arranger Midi DAW.app to Applications.
Apple Silicon and Intel. Ad-hoc signed, not notarized by Apple: the first time,
right-click the app and choose Open, then Open again." > "$stage/INSTALL.txt"
    file="$name-macos-universal.dmg"
    hdiutil create -volname "Lazy Arranger ${tag#v}" -srcfolder "$stage" \
      -format UDZO -fs HFS+ "$out/$file"
    hdiutil verify "$out/$file"
    ;;
  windows)
    dir="$stage/$name-windows-x64"
    mkdir -p "$dir"
    cp target/release/lazy-arranger-desktop.exe "$dir/Lazy Arranger Midi DAW.exe"
    cp target/release/lazy-arranger-plugin-scan.exe "$dir/"
    cp target/release/lazy-arranger.exe "$dir/"
    cp LICENSE apps/desktop/assets/fonts/Inter-LICENSE.txt "$dir/"
    notes "Unzip the folder anywhere and run \"Lazy Arranger Midi DAW.exe\".
Not code-signed: if Windows SmartScreen warns, click More info > Run anyway.
lazy-arranger.exe is the command-line tool." > "$dir/INSTALL.txt"
    file="$name-windows-x64.zip"
    (cd "$stage" && 7z a -tzip -bd "$out/$file" "$name-windows-x64" >/dev/null)
    ;;
  linux)
    dir="$stage/$name-linux-x64"
    mkdir -p "$dir"
    cp target/release/lazy-arranger-desktop target/release/lazy-arranger-plugin-scan \
      target/release/lazy-arranger "$dir/"
    cp LICENSE apps/desktop/assets/fonts/Inter-LICENSE.txt "$dir/"
    cp apps/desktop/assets/app-icon-256.png "$dir/lazy-arranger.png"
    cat > "$dir/install.sh" <<'SH'
#!/bin/sh
# Adds Lazy Arranger to the app menu for this user (no root needed).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
mkdir -p "$apps" "$icons"
cp "$here/lazy-arranger.png" "$icons/lazy-arranger.png"
cat > "$apps/lazy-arranger.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Lazy Arranger Midi DAW
Comment=MIDI arranger and DAW
Exec="$here/lazy-arranger-desktop" %f
Icon=lazy-arranger
Categories=AudioVideo;Audio;Midi;
Terminal=false
DESKTOP
echo "Added Lazy Arranger to the app menu."
SH
    chmod +x "$dir/install.sh"
    notes "Extract the folder anywhere and run ./lazy-arranger-desktop.
Run ./install.sh once to add it to your app menu.
Needs ALSA (libasound2), and X11 or Wayland with Vulkan or OpenGL drivers.
lazy-arranger is the command-line tool." > "$dir/INSTALL.txt"
    file="$name-linux-x64.tar.gz"
    tar -C "$stage" -czf "$out/$file" "$name-linux-x64"
    ;;
  *)
    echo "unknown platform: $platform" >&2
    exit 1
    ;;
esac

if command -v sha256sum >/dev/null; then
  (cd "$out" && sha256sum "$file" > "$file.sha256")
else
  (cd "$out" && shasum -a 256 "$file" > "$file.sha256")
fi
echo "packed $out/$file"
