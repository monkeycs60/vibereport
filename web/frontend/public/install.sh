#!/bin/sh
# vibereport installer — https://vibereport.dev
# Usage: curl -fsSL https://vibereport.dev/install.sh | sh
set -e

REPO="vibereport/vibereport"
BINARY="vibereport"

# Detect OS
OS=$(uname -s)
case "$OS" in
  Linux)  OS_TAG="unknown-linux-gnu" ;;
  Darwin) OS_TAG="apple-darwin" ;;
  *)      echo "Error: Unsupported OS: $OS"; exit 1 ;;
esac

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
  x86_64|amd64)   ARCH_TAG="x86_64" ;;
  arm64|aarch64)
    if [ "$OS" = "Darwin" ]; then
      ARCH_TAG="aarch64"
    else
      echo "Error: Linux ARM64 builds are not yet available."; exit 1
    fi
    ;;
  *)  echo "Error: Unsupported architecture: $ARCH"; exit 1 ;;
esac

TARGET="${ARCH_TAG}-${OS_TAG}"

# Fetch latest release tag
echo "Fetching latest release..."
LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')
if [ -z "$LATEST" ]; then
  echo "Error: Could not determine latest release."; exit 1
fi
echo "Latest release: ${LATEST}"

# Download
URL="https://github.com/${REPO}/releases/download/${LATEST}/${BINARY}-${TARGET}.tar.gz"
echo "Downloading ${BINARY}-${TARGET}.tar.gz..."
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
curl -fsSL "$URL" -o "${TMPDIR}/release.tar.gz"

# Extract
tar xzf "${TMPDIR}/release.tar.gz" -C "$TMPDIR"

# Install
if [ -w /usr/local/bin ]; then
  INSTALL_DIR="/usr/local/bin"
else
  INSTALL_DIR="${HOME}/.local/bin"
  mkdir -p "$INSTALL_DIR"
fi

cp "${TMPDIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
chmod +x "${INSTALL_DIR}/${BINARY}"

echo ""
echo "vibereport ${LATEST} installed to ${INSTALL_DIR}/${BINARY}"

# PATH check
case ":$PATH:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo ""
    echo "Warning: ${INSTALL_DIR} is not in your PATH."
    echo "Add it with:  export PATH=\"${INSTALL_DIR}:\$PATH\""
    ;;
esac

echo ""
echo "Run 'vibereport' in any git repo to get started."
