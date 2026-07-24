#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/signing_identity.sh"
identity="$(copets_signing_identity)"
keychain="${HOME}/Library/Keychains/login.keychain-db"

if ! security find-identity -v -p codesigning "$keychain" | grep -Fq "\"${identity}\""; then
  echo "Missing valid local code-signing identity: ${identity}" >&2
  echo "Run: npm run codesign:setup" >&2
  exit 1
fi

export APPLE_SIGNING_IDENTITY="$identity"
exec npm run tauri -- build "$@"
