#!/usr/bin/env sh
set -eu

REPO="abhinavm-web/jitpipe"
PKG_NAME="jitpipe"
BINS="jitpipe sgrep sawk sjq"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}:${ARCH}" in
  Linux:x86_64|Linux:amd64)
    TARGET="x86_64-unknown-linux-gnu"
    ;;
  Darwin:arm64|Darwin:aarch64)
    TARGET="aarch64-apple-darwin"
    ;;
  *)
    echo "Unsupported prebuilt target: ${OS} ${ARCH}" >&2
    echo "Build from source instead: cargo install --git https://github.com/${REPO}" >&2
    exit 1
    ;;
esac

ARCHIVE="${PKG_NAME}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/latest/download/${ARCHIVE}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT INT TERM

echo "Downloading ${PKG_NAME} (${TARGET}) from ${URL}..."
curl -fsSL "${URL}" -o "${TMP_DIR}/${ARCHIVE}"
tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "${TMP_DIR}"

mkdir -p "${INSTALL_DIR}"
for bin in ${BINS}; do
  install -m 755 "${TMP_DIR}/${bin}" "${INSTALL_DIR}/${bin}"
  echo "Installed ${bin} to ${INSTALL_DIR}/${bin}"
done

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo "Add ${INSTALL_DIR} to your PATH to run jitpipe utilities from anywhere:"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    ;;
esac
