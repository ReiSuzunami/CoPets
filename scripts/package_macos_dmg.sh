#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
source "$repo_root/scripts/signing_identity.sh"

app_name="CoPets"
version="$(node -p "require('${repo_root}/package.json').version")"
identity="$(copets_signing_identity)"
installer_name="Install ${app_name}.app"
volume_name="${app_name} ${version}"
minimum_macos="11.0"
artifact_dir="${COPETS_ARTIFACT_DIR:-${repo_root}/artifacts/release/v${version}}"
artifact_path="${artifact_dir}/${app_name}-v${version}-macos-universal.dmg"
keychain="${HOME}/Library/Keychains/login.keychain-db"
work_dir="$(mktemp -d -t copets-dmg.XXXXXX)"
mounted_path=""

cleanup() {
  if [[ -n "$mounted_path" && -d "$mounted_path" ]]; then
    hdiutil detach "$mounted_path" >/dev/null 2>&1 ||
      hdiutil detach -force "$mounted_path" >/dev/null 2>&1 || true
  fi
  rm -rf "$work_dir"
}
trap cleanup EXIT

[[ "$(uname -s)" == "Darwin" ]] || {
  echo "error: macOS DMG packaging requires macOS" >&2
  exit 2
}
for command in codesign hdiutil lipo node npm osascript plutil python3 rustup security shasum swiftc xcrun; do
  command -v "$command" >/dev/null || {
    echo "error: required command not found: $command" >&2
    exit 2
  }
done
if ! security find-identity -v -p codesigning "$keychain" | grep -Fq "\"${identity}\""; then
  echo "error: missing valid signing identity: ${identity}" >&2
  echo "run: npm run codesign:setup" >&2
  exit 1
fi
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  if ! rustup target list --installed | grep -Fxq "$target"; then
    echo "error: missing Rust target: $target" >&2
    echo "run: rustup target add $target" >&2
    exit 1
  fi
done

export MACOSX_DEPLOYMENT_TARGET="$minimum_macos"
if [[ "${COPETS_PACKAGE_SKIP_APP_BUILD:-0}" == "1" ]]; then
  echo "warning: reusing the existing universal app bundle" >&2
else
  "$repo_root/scripts/build_macos_signed.sh" \
    --target universal-apple-darwin \
    --bundles app
fi

payload="$repo_root/src-tauri/target/universal-apple-darwin/release/bundle/macos/${app_name}.app"
[[ -d "$payload" ]] || {
  echo "error: universal app bundle missing: $payload" >&2
  exit 1
}

installer_app="$work_dir/$installer_name"
installer_contents="$installer_app/Contents"
installer_binary="$installer_contents/MacOS/install-copets"
mkdir -p \
  "$installer_contents/MacOS" \
  "$installer_contents/Resources" \
  "$installer_contents/Helpers"

cat >"$installer_contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>Install CoPets</string>
  <key>CFBundleExecutable</key>
  <string>install-copets</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>dev.copets.installer</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Install CoPets</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>${minimum_macos}</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST
plutil -lint "$installer_contents/Info.plist" >/dev/null

swift_source="$repo_root/installer/macos/Installer.swift"
swiftc \
  -target "arm64-apple-macosx${minimum_macos}" \
  -O \
  -framework AppKit \
  "$swift_source" \
  -o "$work_dir/install-copets-arm64"
swiftc \
  -target "x86_64-apple-macosx${minimum_macos}" \
  -O \
  -framework AppKit \
  "$swift_source" \
  -o "$work_dir/install-copets-x86_64"
lipo -create \
  "$work_dir/install-copets-arm64" \
  "$work_dir/install-copets-x86_64" \
  -output "$installer_binary"
chmod 0755 "$installer_binary"

cp "$repo_root/src-tauri/icons/icon.icns" "$installer_contents/Resources/AppIcon.icns"
ditto "$payload" "$installer_contents/Helpers/${app_name}.app"

codesign --force --timestamp=none --sign "$identity" "$installer_binary"
codesign --force --timestamp=none --sign "$identity" "$installer_app"
codesign --verify --deep --strict --verbose=2 "$installer_app"

"$repo_root/scripts/test_macos_installer.sh" \
  "$installer_binary" \
  "$installer_contents/Helpers/${app_name}.app"

dmg_root="$work_dir/dmg-root"
mkdir -p "$dmg_root/.background"
ditto "$installer_app" "$dmg_root/$installer_name"
xcrun swift "$repo_root/scripts/render_dmg_background.swift" \
  "$repo_root/docs/assets/brand/copets-cloud.png" \
  "$dmg_root/.background/background.png"
touch "$dmg_root/.metadata_never_index"

read_write_dmg="$work_dir/${app_name}-layout.dmg"
read_write_size_kib="$(
  du -sk "$dmg_root" | awk '{ value = $1 + 65536; if (value < 131072) value = 131072; print value }'
)"
hdiutil create \
  -quiet \
  -size "${read_write_size_kib}k" \
  -fs HFS+ \
  -volname "$volume_name" \
  -srcfolder "$dmg_root" \
  -format UDRW \
  "$read_write_dmg"

attach_plist="$work_dir/layout-attach.plist"
hdiutil attach \
  -readwrite \
  -noverify \
  -noautoopen \
  -plist \
  "$read_write_dmg" >"$attach_plist"
mounted_path="$(
  plutil -convert json -o - "$attach_plist" |
    node -e '
      let input = "";
      process.stdin.on("data", (chunk) => { input += chunk; });
      process.stdin.on("end", () => {
        const plist = JSON.parse(input);
        const entity = plist["system-entities"].find((entry) => entry["mount-point"]);
        if (!entity) process.exit(1);
        process.stdout.write(entity["mount-point"]);
      });
    '
)"
[[ -d "$mounted_path" ]] || {
  echo "error: hdiutil did not return a mounted volume" >&2
  exit 1
}
python3 "$repo_root/scripts/write_dmg_ds_store.py" \
  "$mounted_path" \
  "$installer_name"

osascript <<APPLESCRIPT
tell application "Finder"
  tell disk "$volume_name"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set pathbar visible of container window to false
    set bounds of container window to {120, 120, 840, 560}
    set theViewOptions to the icon view options of container window
    set arrangement of theViewOptions to not arranged
    set icon size of theViewOptions to 112
    set text size of theViewOptions to 13
    set background picture of theViewOptions to file ".background:background.png"
    set position of item "$installer_name" of container window to {360, 318}
    update without registering applications
    delay 2
    close
  end tell
end tell
APPLESCRIPT
sync
for _ in {1..20}; do
  [[ -s "$mounted_path/.DS_Store" ]] && break
  sleep 0.5
done
[[ -s "$mounted_path/.DS_Store" ]] || {
  echo "error: Finder did not persist the DMG layout .DS_Store" >&2
  exit 1
}
hdiutil detach "$mounted_path" >/dev/null
mounted_path=""

mkdir -p "$artifact_dir"
converted_dmg="$work_dir/${app_name}-v${version}-macos-universal.dmg"
hdiutil convert \
  -quiet \
  "$read_write_dmg" \
  -format UDZO \
  -imagekey zlib-level=9 \
  -o "$converted_dmg"
mv -f "$converted_dmg" "$artifact_path"
hdiutil verify "$artifact_path" >/dev/null

"$HOME/.codex/skills/package-self-cleaning-macos-dmg/scripts/audit-dmg.sh" \
  "$artifact_path" \
  "$installer_name" \
  "Contents/Helpers/${app_name}.app" \
  "dev.copets.sidecar" \
  "$minimum_macos"

cleanup_mount="$work_dir/cleanup-mount"
mkdir -p "$cleanup_mount"
hdiutil attach \
  -quiet \
  -nobrowse \
  -readonly \
  -mountpoint "$cleanup_mount" \
  "$artifact_path"
mounted_path="$cleanup_mount"
resolved_image="$(
  COPETS_INSTALLER_TEST_MODE=1 \
    "$installer_binary" \
    --test-resolve-image \
    "$cleanup_mount/$installer_name"
)"
expected_resolution="$(printf '%s\n%s' "$cleanup_mount" "$artifact_path")"
actual_resolution="$(printf '%s\n%s' \
  "$(sed -n '1p' <<<"$resolved_image")" \
  "$(sed -n '2p' <<<"$resolved_image")")"
[[ "$actual_resolution" == "$expected_resolution" ]] || {
  echo "error: mounted image resolution returned unexpected paths" >&2
  printf 'expected:\n%s\nactual:\n%s\n' "$expected_resolution" "$actual_resolution" >&2
  exit 1
}
cleanup_device_entry="$(sed -n '3p' <<<"$resolved_image")"
cleanup_image_device="$(sed -n '4p' <<<"$resolved_image")"
cleanup_image_inode="$(sed -n '5p' <<<"$resolved_image")"
[[ -n "$cleanup_device_entry" && -n "$cleanup_image_device" && -n "$cleanup_image_inode" ]] || {
  echo "error: mounted image identity was incomplete" >&2
  exit 1
}

temporary_root="$(getconf DARWIN_USER_TEMP_DIR)"
cleanup_helper="${temporary_root}copets-installer-cleanup-$(uuidgen)"
cp "$installer_binary" "$cleanup_helper"
chmod 0700 "$cleanup_helper"
sleep 0.5 &
waiter_pid="$!"
"$cleanup_helper" \
  --cleanup-helper \
  "$waiter_pid" \
  "$cleanup_mount" \
  "$artifact_path" \
  "$cleanup_device_entry" \
  "$cleanup_image_device" \
  "$cleanup_image_inode" \
  keep &
helper_pid="$!"
wait "$waiter_pid"
wait "$helper_pid"
mounted_path=""
[[ ! -e "$cleanup_helper" ]] || {
  echo "error: cleanup helper did not remove itself" >&2
  exit 1
}
if mount | grep -Fq "on ${cleanup_mount} "; then
  echo "error: copied cleanup helper did not eject the mounted DMG" >&2
  exit 1
fi
[[ -f "$artifact_path" ]] || {
  echo "error: cleanup helper removed a DMG it was told to keep" >&2
  exit 1
}

checksum_path="${artifact_path}.sha256"
(
  cd "$artifact_dir"
  shasum -a 256 "$(basename "$artifact_path")" >"$(basename "$checksum_path")"
)

size="$(du -h "$artifact_path" | awk '{ print $1 }')"
checksum="$(awk '{ print $1 }' "$checksum_path")"
printf 'artifact: %s\nsize: %s\nsha256: %s\nsigning: development identity (%s), not notarized\n' \
  "$artifact_path" \
  "$size" \
  "$checksum" \
  "$identity"
