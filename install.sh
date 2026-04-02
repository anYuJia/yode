#!/usr/bin/env bash
# Yode installer — downloads a prebuilt binary from GitHub Releases.
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/anYuJia/yode/main/install.sh | bash
#
# Environment variables:
#   INSTALL_DIR  — where to place the binary (default: ~/.local/bin)
#   VERSION      — release tag to install   (default: latest)

set -euo pipefail

REPO="anYuJia/yode"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# ── helpers ──────────────────────────────────────────────────────────
info()  { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33mwarning:\033[0m %s\n' "$*"; }
error() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || error "'$1' is required but not found. Please install it first."
}

# ── detect platform ──────────────────────────────────────────────────
detect_platform() {
  local os arch

  case "$(uname -s)" in
    Linux*)  os="unknown-linux-gnu" ;;
    Darwin*) os="apple-darwin" ;;
    *)       error "Unsupported OS: $(uname -s). Only Linux and macOS are supported." ;;
  esac

  case "$(uname -m)" in
    x86_64|amd64)  arch="x86_64" ;;
    arm64|aarch64)
      if [ "$os" = "apple-darwin" ]; then
        arch="aarch64"
      else
        error "Linux ARM64 builds are not yet available. Only x86_64 is supported on Linux."
      fi
      ;;
    *)             error "Unsupported architecture: $(uname -m). Only x86_64 and aarch64 (macOS) are supported." ;;
  esac

  echo "${arch}-${os}"
}

# ── resolve version ──────────────────────────────────────────────────
resolve_version() {
  if [ -n "${VERSION:-}" ]; then
    echo "$VERSION"
    return
  fi

  need curl
  local latest
  latest=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name":\s*"([^"]+)".*/\1/')

  [ -n "$latest" ] || error "Could not determine latest release. Set VERSION explicitly."
  echo "$latest"
}

# ── main ─────────────────────────────────────────────────────────────
main() {
  need curl
  need tar

  local platform version archive url tmpdir

  platform=$(detect_platform)
  version=$(resolve_version)
  archive="yode-${platform}.tar.gz"
  url="https://github.com/${REPO}/releases/download/${version}/${archive}"

  info "Installing yode ${version} (${platform})"
  info "Download: ${url}"

  tmpdir=$(mktemp -d)
  trap 'rm -rf "$tmpdir"' EXIT

  # download
  if ! curl -fSL --progress-bar -o "${tmpdir}/${archive}" "$url"; then
    error "Download failed. Check that release ${version} exists and has asset ${archive}."
  fi

  # extract
  tar -xzf "${tmpdir}/${archive}" -C "$tmpdir"

  # install
  mkdir -p "$INSTALL_DIR"
  local bin="${tmpdir}/yode"

  # handle tar archives that may contain the binary directly or in a subdirectory
  if [ ! -f "$bin" ]; then
    bin=$(find "$tmpdir" -name "yode" -type f | head -1)
    [ -n "$bin" ] || error "Could not find 'yode' binary in archive."
  fi

  chmod +x "$bin"
  mv "$bin" "${INSTALL_DIR}/yode"

  info "Installed to ${INSTALL_DIR}/yode"

  # verify
  if "${INSTALL_DIR}/yode" --version >/dev/null 2>&1; then
    info "Verification passed: $("${INSTALL_DIR}/yode" --version 2>/dev/null || echo 'yode')"
  else
    warn "Binary installed but could not run --version (may still work)."
  fi

  # PATH hint
  case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
      echo ""
      warn "${INSTALL_DIR} is not in your PATH."
      echo "  Add it by running:"
      echo ""
      echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
      echo ""
      echo "  Or add that line to your ~/.bashrc / ~/.zshrc."
      ;;
  esac

  echo ""
  info "Done! Run 'yode' to get started."
}

main "$@"
