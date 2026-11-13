#!/usr/bin/env bash
# Signs the update feed, or checks its signature, so the app can tell that a feed came from the release workflow.
# Velopack only checks each package against the checksum the feed gives, so whoever could change the feed could
# change the app. `apps/desktop/src/updates/source/mod.rs` defines the signature, and the form of the public key.
#
#   tools/package/feed-signature.sh sign FEED PRIVATE_KEY PUBLIC_KEY   Write FEED.sig, then check it as verify does
#   tools/package/feed-signature.sh verify FEED PUBLIC_KEY             Check FEED.sig against the key the app holds
#   tools/package/feed-signature.sh public PRIVATE_KEY                 Print the public key the way the app holds it
#   tools/package/feed-signature.sh openssl                            Print the OpenSSL this uses
#
# PRIVATE_KEY is a PKCS#8 PEM file, and PUBLIC_KEY is in the form the app builds in. It needs OpenSSL 3 or newer.
# macOS's own `openssl` is LibreSSL, which can't sign raw input with Ed25519. To make a key, or a new one to replace
# it, run `"$(tools/package/feed-signature.sh openssl)" genpkey -algorithm ed25519 -out KEY.pem`, then `public` on it
# gives what the app builds in.
set -euo pipefail

die() { echo "error: $1" >&2; exit 1; }

# An Ed25519 public key in DER is this header and then the key.
SPKI_HEADER='\x30\x2a\x30\x05\x06\x03\x2b\x65\x70\x03\x21\x00'

find_openssl() {
  local candidate
  for candidate in openssl "$(brew --prefix openssl@3 2> /dev/null)/bin/openssl"; do
    if "$candidate" version 2> /dev/null | awk '$1 == "OpenSSL" { split($2, v, "."); found = v[1] >= 3 } END { exit !found }'; then
      echo "$candidate"
      return
    fi
  done
  die "Ed25519 needs OpenSSL 3 or newer, such as from \`brew install openssl@3\`"
}

# Writes the app's form of a public key as a PEM file OpenSSL can read.
public_key_pem() {
  local base64=$1 out=$2
  { printf '%b' "$SPKI_HEADER"; printf '%s' "$base64" | base64 --decode; } > "$out.der"
  [ "$(wc -c < "$out.der")" -eq 44 ] || die "the public key isn't 32 bytes in base64"
  "$OPENSSL" pkey -pubin -inform DER -in "$out.der" -out "$out"
}

# Prints a private key's public key the way the app holds it, the DER without its header.
public() {
  local private=$1
  "$OPENSSL" pkey -in "$private" -pubout -outform DER -out "$WORK/public.der"
  printf '%b' "$SPKI_HEADER" > "$WORK/header"
  if [ "$(wc -c < "$WORK/public.der")" -ne 44 ] || ! head -c 12 "$WORK/public.der" | cmp -s - "$WORK/header"; then
    die "$private isn't an Ed25519 key"
  fi
  tail -c 32 "$WORK/public.der" | base64
}

verify() {
  local feed=$1 public=$2
  [ -f "$feed.sig" ] || die "$feed.sig is missing"
  [ "$(wc -c < "$feed.sig")" -eq 64 ] || die "$feed.sig isn't a 64-byte signature"
  public_key_pem "$public" "$WORK/public.pem"
  "$OPENSSL" pkeyutl -verify -pubin -inkey "$WORK/public.pem" -rawin -in "$feed" -sigfile "$feed.sig" > /dev/null \
    || die "$feed.sig doesn't match $feed and that public key"
}

sign() {
  local feed=$1 private=$2 public=$3
  "$OPENSSL" pkeyutl -sign -inkey "$private" -rawin -in "$feed" -out "$feed.sig"
  # A key that doesn't match the app's would publish a feed every app refuses.
  verify "$feed" "$public"
}

OPENSSL="$(find_openssl)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
case "${1:-}" in
  sign) [ $# -eq 4 ] || die "sign needs FEED PRIVATE_KEY PUBLIC_KEY"; sign "$2" "$3" "$4" ;;
  verify) [ $# -eq 3 ] || die "verify needs FEED PUBLIC_KEY"; verify "$2" "$3" ;;
  public) [ $# -eq 2 ] || die "public needs PRIVATE_KEY"; public "$2" ;;
  openssl) [ $# -eq 1 ] || die "openssl takes nothing"; echo "$OPENSSL" ;;
  *) die "unknown command ${1:-}. See the top of this script" ;;
esac
