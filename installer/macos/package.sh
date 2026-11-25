#!/usr/bin/env bash
# Builds EQaura.app and a .dmg. Usage: installer/macos/package.sh 0.1.0 [binary]
set -euo pipefail
version="$1"
cd "$(dirname "$0")/../.."
binary="${2:-target/release/eqaura}"
app="dist/macos/EQaura.app"
rm -rf dist/macos && mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/EQaura"
cp LICENSE "$app/Contents/Resources/LICENSE.txt"
cp dist/THIRD-PARTY-LICENSES.txt "$app/Contents/Resources/"
cp apps/desktop/assets/icon.icns "$app/Contents/Resources/EQaura.icns"
cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>EQaura</string>
<key>CFBundleDisplayName</key><string>EQaura</string>
<key>CFBundleIdentifier</key><string>io.github.adityatummuri.eqaura</string>
<key>CFBundleExecutable</key><string>EQaura</string>
<key>CFBundleIconFile</key><string>EQaura</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>LSMinimumSystemVersion</key><string>14.2</string>
<key>LSApplicationCategoryType</key><string>public.app-category.music</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSAudioCaptureUsageDescription</key><string>EQaura needs the system sound to equalize it. Nothing is recorded or sent anywhere.</string>
</dict></plist>
EOF
# Ad-hoc signature: required on Apple silicon, does not identify a developer.
codesign --force --sign - --identifier io.github.adityatummuri.eqaura "$app"
codesign --verify --strict "$app"
ln -s /Applications dist/macos/Applications
hdiutil create -quiet -volname EQaura -srcfolder dist/macos -ov -format ULMO "dist/EQaura-$version-macos.dmg"
