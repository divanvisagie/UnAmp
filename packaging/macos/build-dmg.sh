#!/usr/bin/env bash
# Builds UnAmp.app as a universal (Apple Silicon + Intel) binary and wraps it
# in a drag-to-Applications .dmg. Runs on macOS: the release workflow calls
# it through `make dmg` (see ADR-0025). The app is ad-hoc signed only, not
# notarised, so Gatekeeper asks the user to allow it on first launch.
#
# Output: target/dmg/UnAmp-<version>.dmg and an unversioned UnAmp.dmg copy
# for https://github.com/divanvisagie/UnAmp/releases/latest/download/UnAmp.dmg
set -euo pipefail

cd "$(dirname "$0")/../.."
version="$(awk -F\" '/^version = / { print $2; exit }' Cargo.toml)"
out=target/dmg
app="$out/UnAmp.app"
targets=(aarch64-apple-darwin x86_64-apple-darwin)

for t in "${targets[@]}"; do
  rustup target add "$t" >/dev/null
  cargo build --release --locked --bin unamp --target "$t"
done

rm -rf "$out"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/unamp" \
  target/aarch64-apple-darwin/release/unamp target/x86_64-apple-darwin/release/unamp
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"

# The icon: the SVG rendered at every size an .icns holds.
iconset="$out/unamp.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  rsvg-convert -w "$size" -h "$size" packaging/linux/unamp.svg -o "$iconset/icon_${size}x${size}.png"
  rsvg-convert -w $((size * 2)) -h $((size * 2)) packaging/linux/unamp.svg -o "$iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/unamp.icns"
rm -rf "$iconset"

# Apple Silicon refuses to run unsigned code; an ad-hoc signature is enough
# to launch, though not to pass Gatekeeper without the user's say-so.
codesign --force --deep --sign - "$app"

# The disk image: the app beside an Applications shortcut to drag it onto.
stage="$out/stage"
mkdir -p "$stage"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -volname "UnAmp $version" -srcfolder "$stage" -fs HFS+ -format UDZO -ov "$out/UnAmp-$version.dmg"
rm -rf "$stage"
cp "$out/UnAmp-$version.dmg" "$out/UnAmp.dmg"
echo "Built $out/UnAmp-$version.dmg"
