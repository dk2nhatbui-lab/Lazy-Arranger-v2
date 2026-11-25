#!/bin/sh
# Builds a minimal macOS .app around the desktop binary.
#
#   ./scripts/bundle-macos.sh              # this Mac's architecture
#   UNIVERSAL=1 ./scripts/bundle-macos.sh  # Apple Silicon + Intel (release CI)
set -eu
cd "$(dirname "$0")/.."

BINS="lazy-arranger-desktop lazy-arranger-plugin-scan"
OUT=target/bundle-bin
rm -rf "$OUT"
mkdir -p "$OUT"
if [ "${UNIVERSAL:-0}" = "1" ]; then
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    ./scripts/rust.sh build --release --target "$target" -p lazy-arranger-desktop
    ./scripts/rust.sh build --release --target "$target" -p plugin-host --bin lazy-arranger-plugin-scan
  done
  for bin in $BINS; do
    lipo -create -output "$OUT/$bin" \
      "target/aarch64-apple-darwin/release/$bin" \
      "target/x86_64-apple-darwin/release/$bin"
  done
else
  ./scripts/rust.sh build --release -p lazy-arranger-desktop
  ./scripts/rust.sh build --release -p plugin-host --bin lazy-arranger-plugin-scan
  for bin in $BINS; do cp "target/release/$bin" "$OUT/$bin"; done
fi

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
# macOS bundle versions must be plain numbers: 0.1.0-beta.1 -> 0.1.0.
SHORT_VERSION=${VERSION%%-*}
APP="dist/Lazy Arranger Midi DAW.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
for bin in $BINS; do cp "$OUT/$bin" "$APP/Contents/MacOS/$bin"; done
cp apps/desktop/assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Lazy Arranger Midi DAW</string>
  <key>CFBundleDisplayName</key><string>Lazy Arranger Midi DAW</string>
  <key>CFBundleIdentifier</key><string>com.lazyarranger.mididaw</string>
  <key>CFBundleExecutable</key><string>lazy-arranger-desktop</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleIconName</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>__SHORT_VERSION__</string>
  <key>CFBundleVersion</key><string>__SHORT_VERSION__</string>
  <key>CFBundleGetInfoString</key><string>__VERSION__</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key>
  <string>Records audio takes and dictation into the song.</string>
  <key>NSAudioCaptureUsageDescription</key>
  <string>Records audio takes into the song.</string>
</dict>
</plist>
PLIST

sed -i '' -e "s/__SHORT_VERSION__/$SHORT_VERSION/g" -e "s/__VERSION__/$VERSION/g" \
  "$APP/Contents/Info.plist"
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"
echo "built $APP"
