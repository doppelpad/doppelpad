#!/bin/sh
# Makes the self-signed certificate the release workflow signs the macOS app with, until there
# is an Apple Developer ID (see docs/MACOS.md#signing-releases). Run it once, on a Mac:
#
#   sh gui/scripts/macos-signing-certificate.sh [directory]
#
# It writes the certificate with its private key (doppelpad-signing.p12) and the file's
# password (password.txt) into the directory, ~/doppelpad-signing by default, and lists the
# repository secrets to set. Keep both files: every release has to be signed with the same
# certificate, or macOS takes the update for another program and asks for its permissions
# again.
set -eu

dir=${1:-$HOME/doppelpad-signing}
name="Doppelpad (self-signed)"
if [ -e "$dir/doppelpad-signing.p12" ]; then
  echo "$dir/doppelpad-signing.p12 exists already: keep signing with it" >&2
  exit 1
fi
mkdir -p "$dir"
chmod 700 "$dir"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# macOS's own openssl (LibreSSL): `security import` reads its PKCS#12 files.
openssl=/usr/bin/openssl
password=$("$openssl" rand -hex 24)
"$openssl" req -x509 -newkey rsa:3072 -nodes -days 7300 \
  -subj "/CN=$name" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" \
  -keyout "$tmp/key.pem" -out "$tmp/cert.pem" 2>/dev/null
"$openssl" pkcs12 -export -name "$name" -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
  -out "$dir/doppelpad-signing.p12" -passout "pass:$password"
printf '%s\n' "$password" > "$dir/password.txt"
chmod 600 "$dir/doppelpad-signing.p12" "$dir/password.txt"

cat <<EOF
Wrote $dir/doppelpad-signing.p12 and $dir/password.txt; the certificate is valid for 20 years.
Keep both safe (in a password manager, for example) and out of the repository.

Repository secrets (GitHub: Settings › Secrets and variables › Actions › New repository secret):

  MACOS_CERTIFICATE            base64 -i "$dir/doppelpad-signing.p12" | pbcopy  (then paste)
  MACOS_CERTIFICATE_PASSWORD   the line in $dir/password.txt
  MACOS_SIGNING_IDENTITY       $name
EOF
