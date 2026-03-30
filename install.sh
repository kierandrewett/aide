#!/usr/bin/env bash
set -euo pipefail

REPO="kierandrewett/aide"
INSTALL_DIR="${AIDE_INSTALL_DIR:-$HOME/.local/bin}"

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  PLATFORM="linux" ;;
    Darwin) PLATFORM="macos" ;;
    *)
        echo "Error: Unsupported OS: $OS"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64)  ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *)
        echo "Error: Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

ARCHIVE="aide-${ARCH}-${PLATFORM}.tar.gz"

# Get latest release tag
echo "Fetching latest release..."
TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)

if [ -z "$TAG" ]; then
    echo "Error: Could not determine latest release"
    exit 1
fi

echo "Installing aide ${TAG} (${ARCH}-${PLATFORM})..."

# Download
URL="https://github.com/${REPO}/releases/download/${TAG}/${ARCHIVE}"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading ${URL}..."
curl -fsSL "$URL" -o "${TMPDIR}/${ARCHIVE}"

# Extract
tar xzf "${TMPDIR}/${ARCHIVE}" -C "$TMPDIR"

# Install
mkdir -p "$INSTALL_DIR"
mv "${TMPDIR}/aide" "${INSTALL_DIR}/aide"
chmod +x "${INSTALL_DIR}/aide"

echo ""
echo "aide installed to ${INSTALL_DIR}/aide"

# Check PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    echo ""
    echo "WARNING: ${INSTALL_DIR} is not in your PATH."
    echo "Add it with:"
    echo ""
    echo "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.bashrc"
    echo ""
fi

echo "Run 'aide' to get started."
