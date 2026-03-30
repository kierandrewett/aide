#!/usr/bin/env bash
set -euo pipefail

REPO="kierandrewett/aide"
INSTALL_DIR="${AIDE_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${1:-latest}"

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

# Resolve version
if [ "$VERSION" = "latest" ]; then
    echo "Fetching latest release..."
    TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name"' | cut -d'"' -f4 || true)
    if [ -z "$TAG" ]; then
        echo "Error: No releases found. Check https://github.com/${REPO}/releases"
        exit 1
    fi
else
    TAG="$VERSION"
    # Prefix with v if needed
    [[ "$TAG" == v* ]] || TAG="v${TAG}"
fi

echo "Installing aide ${TAG} (${ARCH}-${PLATFORM})..."

# Download
URL="https://github.com/${REPO}/releases/download/${TAG}/${ARCHIVE}"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading ${URL}..."
if ! curl -fsSL "$URL" -o "${TMPDIR}/${ARCHIVE}"; then
    echo "Error: Failed to download ${ARCHIVE} for ${TAG}"
    echo "Check available releases at https://github.com/${REPO}/releases"
    exit 1
fi

# Extract
tar xzf "${TMPDIR}/${ARCHIVE}" -C "$TMPDIR"

# Install
mkdir -p "$INSTALL_DIR"
mv "${TMPDIR}/aide" "${INSTALL_DIR}/aide"
chmod +x "${INSTALL_DIR}/aide"

echo ""
echo "aide ${TAG} installed to ${INSTALL_DIR}/aide"

# Check PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    echo ""
    echo "Note: ${INSTALL_DIR} is not in your PATH. Add it with:"
    echo ""
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo ""
fi

echo "Run 'aide' to get started."
