#!/bin/zsh
# Creates a self-signed code-signing certificate for TVMaster on THIS Mac.
#
# What it gives you: a stable signing identity, so macOS keeps the app's
# permissions (local network, keychain access) across rebuilds instead of
# asking again every time, and `codesign --verify` passes.
# What it does not give you: other people's Macs will still refuse to open the
# app without right-click > Open, because only Apple can vouch for a developer
# you have never met (a paid Developer ID plus notarization).
#
#   Scripts/make-selfsigned-cert.sh                 into your login keychain
#   KEYCHAIN=/tmp/test.keychain-db KEYCHAIN_PASSWORD=x Scripts/make-selfsigned-cert.sh   (for testing)
# Then: SIGN_IDENTITY="TVMaster Self-Signed" Scripts/build-app.sh
set -euo pipefail

NAME="${CERT_NAME:-TVMaster Self-Signed}"
KC="${KEYCHAIN:-$HOME/Library/Keychains/login.keychain-db}"

if security find-identity -p codesigning "$KC" 2>/dev/null | grep -q "$NAME"; then
  echo "\"$NAME\" already exists in $KC"; exit 0
fi

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
cat > "$TMP/cert.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$TMP/cert.cnf" \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" 2>/dev/null
PASS="tvmaster-$$"
# Old-style encryption on purpose: macOS cannot import the AES-based PKCS#12 that OpenSSL 3 writes by default.
openssl pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -out "$TMP/id.p12" -passout "pass:$PASS" \
  -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 2>/dev/null

security import "$TMP/id.p12" -k "$KC" -P "$PASS" -T /usr/bin/codesign >/dev/null
# Let codesign use the key without a prompt each time.
if [[ -n "${KEYCHAIN_PASSWORD:-}" ]]; then
  security set-key-partition-list -S apple-tool:,apple: -s -k "$KEYCHAIN_PASSWORD" "$KC" >/dev/null
fi
echo "Created \"$NAME\" in $KC."
echo "macOS may ask you to trust it: open Keychain Access, double-click the certificate, Trust > Code Signing > Always Trust."
