#!/bin/sh
# Builds a minimal macOS .app around the desktop binary.
set -eu
cd "$(dirname "$0")/.."

./scripts/rust.sh build --release -p lazy-arranger-desktop
./scripts/rust.sh build --release -p plugin-host --bin lazy-arranger-plugin-scan

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
APP="dist/Lazy Arranger Midi DAW.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/lazy-arranger-desktop "$APP/Contents/MacOS/lazy-arranger-desktop"
cp target/release/lazy-arranger-plugin-scan "$APP/Contents/MacOS/lazy-arranger-plugin-scan"
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
  <key>CFBundleShortVersionString</key><string>__VERSION__</string>
  <key>CFBundleVersion</key><string>__VERSION__</string>
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

sed -i '' "s/__VERSION__/$VERSION/g" "$APP/Contents/Info.plist"
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
echo "built $APP"
