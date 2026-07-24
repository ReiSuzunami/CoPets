#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 <installer-executable> <signed-payload.app>" >&2
  exit 2
}

[[ $# -eq 2 ]] || usage
[[ "$(uname -s)" == "Darwin" ]] || {
  echo "error: macOS installer tests require macOS" >&2
  exit 2
}

installer="$1"
payload="$2"
[[ -x "$installer" ]] || {
  echo "error: installer executable missing: $installer" >&2
  exit 2
}
[[ -d "$payload" ]] || {
  echo "error: signed payload missing: $payload" >&2
  exit 2
}

test_root="$(mktemp -d -t copets-installer-tests.XXXXXX)"
applications="$test_root/Applications"
trash="$test_root/Trash"
conflict_root="$test_root/ConflictApplications"
symlink_root="$test_root/SymlinkApplications"
symlink_parent="$test_root/SymlinkParent"
corrupt_root="$test_root/CorruptApplications"
corrupt_payload="$test_root/CorruptCoPets.app"

cleanup() {
  rm -rf "$test_root"
}
trap cleanup EXIT

mkdir -p "$applications" "$trash" "$conflict_root" "$symlink_root" "$corrupt_root"

run_installer() {
  COPETS_INSTALLER_TEST_MODE=1 "$installer" "$@"
}

run_installer --test-install "$payload" "$applications" >/dev/null
codesign --verify --deep --strict --verbose=2 "$applications/CoPets.app"

xattr -w dev.copets.installer-test-upgrade-marker old "$applications/CoPets.app"
xattr -p dev.copets.installer-test-upgrade-marker "$applications/CoPets.app" >/dev/null
run_installer --test-install "$payload" "$applications" >/dev/null
if xattr -p dev.copets.installer-test-upgrade-marker "$applications/CoPets.app" >/dev/null 2>&1; then
  echo "error: second install did not replace the recognized app" >&2
  exit 1
fi

mkdir "$conflict_root/CoPets.app"
touch "$conflict_root/CoPets.app/preserve-marker"
if run_installer --test-install "$payload" "$conflict_root" >/dev/null 2>&1; then
  echo "error: unrecognized CoPets.app was accepted" >&2
  exit 1
fi
[[ -f "$conflict_root/CoPets.app/preserve-marker" ]]

ln -s "$payload" "$symlink_root/CoPets.app"
if run_installer --test-install "$payload" "$symlink_root" >/dev/null 2>&1; then
  echo "error: symlink CoPets.app was accepted" >&2
  exit 1
fi
[[ -L "$symlink_root/CoPets.app" ]]

ln -s "$applications" "$symlink_parent"
if run_installer --test-install "$payload" "$symlink_parent" >/dev/null 2>&1; then
  echo "error: symlink Applications parent was accepted" >&2
  exit 1
fi
[[ -L "$symlink_parent" ]]

ditto "$payload" "$corrupt_payload"
touch "$corrupt_payload/unsigned-marker"
if run_installer --test-install "$corrupt_payload" "$corrupt_root" >/dev/null 2>&1; then
  echo "error: corrupt payload was accepted" >&2
  exit 1
fi
[[ ! -e "$corrupt_root/CoPets.app" ]]

COPETS_INSTALLER_TEST_MODE=1 \
  COPETS_INSTALLER_TEST_TRASH="$trash" \
  "$installer" --test-uninstall "$applications/CoPets.app" ignored >/dev/null
[[ ! -e "$applications/CoPets.app" ]]
codesign --verify --deep --strict --verbose=2 "$trash/CoPets.app"

echo "macOS installer behavior tests passed"
