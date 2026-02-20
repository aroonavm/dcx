#!/bin/sh
set -e

REPO="aroonavm/dcx"
BIN_DIR="${DCX_BIN_DIR:-/usr/local/bin}"

OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Linux)  os="unknown-linux-gnu" ;;
  Darwin) os="apple-darwin" ;;
  *) echo "error: unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64)        arch="x86_64" ;;
  aarch64|arm64) arch="aarch64" ;;
  *) echo "error: unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

TARGET="${arch}-${os}"

VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' \
  | grep -o '"v[^"]*"' \
  | tr -d '"')

if [ -z "$VERSION" ]; then
  echo "error: could not determine latest version" >&2
  exit 1
fi

ARCHIVE="dcx-${VERSION}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"

echo "Installing dcx ${VERSION} for ${TARGET} into ${BIN_DIR}..."

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$URL" -o "$TMP/$ARCHIVE"
tar -xzf "$TMP/$ARCHIVE" -C "$TMP"

if [ -w "$BIN_DIR" ]; then
  install -m 755 "$TMP/dcx" "$BIN_DIR/dcx"
else
  sudo install -m 755 "$TMP/dcx" "$BIN_DIR/dcx"
fi

echo "Done. Run 'dcx --help' to get started."
