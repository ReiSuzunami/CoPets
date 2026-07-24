#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/signing_identity.sh"
identity="$(copets_signing_identity)"
keychain="${HOME}/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning "$keychain" | grep -Fq "\"${identity}\""; then
  echo "Code-signing identity is already ready: ${identity}"
  exit 0
fi

if security find-certificate -a -c "$identity" "$keychain" | grep -q '"alis"'; then
  echo "A certificate named '${identity}' exists but is not a valid code-signing identity." >&2
  echo "Remove or repair that exact certificate in Keychain Access, then retry." >&2
  exit 1
fi

work_dir="$(mktemp -d -t copets-signing.XXXXXX)"
key_file="${work_dir}/codesign.key.pem"
cert_file="${work_dir}/codesign.cert.pem"
p12_file="${work_dir}/codesign.p12"
p12_password="$(openssl rand -hex 24)"
umask 077

cleanup() {
  rm -f "$key_file" "$cert_file" "$p12_file"
  rmdir "$work_dir" 2>/dev/null || true
}
trap cleanup EXIT

openssl req \
  -x509 \
  -newkey rsa:3072 \
  -sha256 \
  -days 825 \
  -nodes \
  -keyout "$key_file" \
  -out "$cert_file" \
  -subj "/CN=${identity}/O=Local Development" \
  -addext 'subjectKeyIdentifier=hash' \
  -addext 'authorityKeyIdentifier=keyid:always,issuer' \
  -addext 'basicConstraints=critical,CA:false' \
  -addext 'keyUsage=critical,digitalSignature' \
  -addext 'extendedKeyUsage=critical,codeSigning'

openssl pkcs12 \
  -export \
  -legacy \
  -inkey "$key_file" \
  -in "$cert_file" \
  -out "$p12_file" \
  -name "$identity" \
  -passout "pass:${p12_password}"

security import "$p12_file" \
  -k "$keychain" \
  -f pkcs12 \
  -x \
  -P "$p12_password" \
  -T /usr/bin/codesign \
  -T /usr/bin/security

# User-domain trust only, restricted to the Code Signing policy.
security add-trusted-cert \
  -r trustRoot \
  -p codeSign \
  -k "$keychain" \
  "$cert_file"

security verify-cert -p codeSign -c "$cert_file"
security find-identity -v -p codesigning "$keychain" | grep -F "\"${identity}\""
echo "Code-signing identity is ready: ${identity}"
