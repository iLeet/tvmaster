#!/usr/bin/env bash
# Signs a Windows installer (or exe) with Authenticode, from Linux or macOS, using osslsigncode.
#
#   packaging/windows/sign.sh <file.exe> --selfsigned
#       makes a self-signed certificate (once, into packaging/windows/selfsigned/) and signs with it.
#       Windows will show "Unknown publisher" and SmartScreen will still warn, UNLESS you first
#       import selfsigned/TVMaster-selfsigned.cer into "Trusted Root Certification Authorities"
#       and "Trusted Publishers" on that PC (certmgr.msc, or in an elevated PowerShell:
#         Import-Certificate -FilePath TVMaster-selfsigned.cer -CertStoreLocation Cert:\LocalMachine\Root
#         Import-Certificate -FilePath TVMaster-selfsigned.cer -CertStoreLocation Cert:\LocalMachine\TrustedPublisher )
#       That is fine for your own PCs. For other people's PCs you need a certificate from a
#       certificate authority (or Azure Trusted Signing).
#
#   WINDOWS_CERT_PFX=cert.pfx WINDOWS_CERT_PASSWORD=... packaging/windows/sign.sh <file.exe>
#       signs with a real certificate.
set -euo pipefail

FILE="$1"; MODE="${2:-}"
HERE="$(cd "$(dirname "$0")" && pwd)"
TS="http://timestamp.digicert.com"

if [[ "$MODE" == "--selfsigned" ]]; then
  DIR="$HERE/selfsigned"; mkdir -p "$DIR"
  if [[ ! -f "$DIR/TVMaster-selfsigned.pfx" ]]; then
    cat > "$DIR/cert.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = TVMaster (self-signed)
O = TVMaster
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
    openssl req -x509 -newkey rsa:3072 -sha256 -nodes -days 3650 -config "$DIR/cert.cnf" \
      -keyout "$DIR/key.pem" -out "$DIR/TVMaster-selfsigned.crt" 2>/dev/null
    openssl pkcs12 -export -inkey "$DIR/key.pem" -in "$DIR/TVMaster-selfsigned.crt" \
      -out "$DIR/TVMaster-selfsigned.pfx" -passout pass:tvmaster 2>/dev/null
    # The public half, to import on the PCs that should trust it.
    openssl x509 -in "$DIR/TVMaster-selfsigned.crt" -outform der -out "$DIR/TVMaster-selfsigned.cer"
    rm -f "$DIR/key.pem" "$DIR/cert.cnf"
    echo "Made $DIR/TVMaster-selfsigned.pfx (keep it private) and TVMaster-selfsigned.cer (hand this one out)"
  fi
  PFX="$DIR/TVMaster-selfsigned.pfx"; PASS="tvmaster"
else
  PFX="${WINDOWS_CERT_PFX:?set WINDOWS_CERT_PFX or pass --selfsigned}"; PASS="${WINDOWS_CERT_PASSWORD:-}"
fi

OUT="$FILE.signed"
osslsigncode sign -pkcs12 "$PFX" -pass "$PASS" -n "TVMaster" -i "https://github.com/iLeet/tvmaster" \
  -h sha256 -ts "$TS" -in "$FILE" -out "$OUT"
mv "$OUT" "$FILE"
echo "Signed $FILE"
