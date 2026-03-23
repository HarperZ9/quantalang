#!/usr/bin/env bash
#
# QuantaLang Installer
# https://quantalang.org
#
# Usage:
#   curl -sSL https://quantalang.org/install.sh | sh
#   curl -sSL https://quantalang.org/install.sh | sh -s -- --version 1.0.0
#   curl -sSL https://quantalang.org/install.sh | sh -s -- --prefix /opt/quanta
#

set -e

# Configuration
VERSION="${QUANTA_VERSION:-latest}"
PREFIX="${QUANTA_PREFIX:-$HOME/.quanta}"
REPO="https://github.com/quantalang/quantalang"
RELEASES="https://releases.quantalang.org"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
info() {
    echo -e "${BLUE}info${NC}: $1"
}

success() {
    echo -e "${GREEN}success${NC}: $1"
}

warn() {
    echo -e "${YELLOW}warning${NC}: $1"
}

error() {
    echo -e "${RED}error${NC}: $1" >&2
    exit 1
}

# Parse arguments
while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            VERSION="$2"
            shift 2
            ;;
        --prefix)
            PREFIX="$2"
            shift 2
            ;;
        --help)
            echo "QuantaLang Installer"
            echo ""
            echo "Usage: install.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --version VERSION  Install specific version (default: latest)"
            echo "  --prefix PATH      Installation directory (default: ~/.quanta)"
            echo "  --help             Show this help message"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"
    
    case "$OS" in
        Linux)
            OS="linux"
            ;;
        Darwin)
            OS="macos"
            ;;
        MINGW*|MSYS*|CYGWIN*)
            OS="windows"
            ;;
        *)
            error "Unsupported operating system: $OS"
            ;;
    esac
    
    case "$ARCH" in
        x86_64|amd64)
            ARCH="x86_64"
            ;;
        aarch64|arm64)
            ARCH="aarch64"
            ;;
        armv7l)
            ARCH="armv7"
            ;;
        *)
            error "Unsupported architecture: $ARCH"
            ;;
    esac
    
    PLATFORM="${ARCH}-${OS}"
    info "Detected platform: $PLATFORM"
}

# Get the latest version from the releases API
get_latest_version() {
    if [ "$VERSION" = "latest" ]; then
        info "Fetching latest version..."
        VERSION=$(curl -sL "$RELEASES/latest" | grep -oP '"tag_name":\s*"v\K[^"]+' || echo "")
        
        if [ -z "$VERSION" ]; then
            # Fallback: check GitHub releases
            VERSION=$(curl -sL "https://api.github.com/repos/quantalang/quantalang/releases/latest" | grep -oP '"tag_name":\s*"v\K[^"]+' || echo "1.0.0")
        fi
    fi
    
    info "Installing QuantaLang v$VERSION"
}

# Download and extract
download() {
    local url="$RELEASES/v$VERSION/quanta-$PLATFORM.tar.gz"
    local tmp_dir=$(mktemp -d)
    local archive="$tmp_dir/quanta.tar.gz"
    
    info "Downloading from $url..."
    
    # Try curl, then wget
    if command -v curl &> /dev/null; then
        curl -fsSL "$url" -o "$archive" || error "Download failed"
    elif command -v wget &> /dev/null; then
        wget -q "$url" -O "$archive" || error "Download failed"
    else
        error "Neither curl nor wget found. Please install one."
    fi
    
    info "Extracting..."
    mkdir -p "$PREFIX"
    tar -xzf "$archive" -C "$PREFIX" --strip-components=1
    
    # Cleanup
    rm -rf "$tmp_dir"
}

# Verify installation
verify() {
    local quanta="$PREFIX/bin/quanta"
    
    if [ ! -f "$quanta" ]; then
        error "Installation failed: quanta binary not found"
    fi
    
    chmod +x "$quanta"
    
    local installed_version=$("$quanta" --version 2>/dev/null | head -1 | grep -oP '\d+\.\d+\.\d+' || echo "unknown")
    info "Installed version: $installed_version"
}

# Update PATH
setup_path() {
    local bin_dir="$PREFIX/bin"
    local profile=""
    
    # Detect shell configuration file
    if [ -n "$BASH_VERSION" ]; then
        if [ -f "$HOME/.bashrc" ]; then
            profile="$HOME/.bashrc"
        elif [ -f "$HOME/.bash_profile" ]; then
            profile="$HOME/.bash_profile"
        fi
    elif [ -n "$ZSH_VERSION" ]; then
        profile="$HOME/.zshrc"
    fi
    
    # Check if already in PATH
    if echo "$PATH" | grep -q "$bin_dir"; then
        info "PATH already configured"
        return
    fi
    
    echo ""
    echo "Add the following to your shell profile ($profile):"
    echo ""
    echo "  export QUANTA_HOME=\"$PREFIX\""
    echo "  export PATH=\"\$QUANTA_HOME/bin:\$PATH\""
    echo ""
    
    # Offer to add automatically
    if [ -n "$profile" ] && [ -t 0 ]; then
        read -p "Add to $profile automatically? [y/N] " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            {
                echo ""
                echo "# QuantaLang"
                echo "export QUANTA_HOME=\"$PREFIX\""
                echo "export PATH=\"\$QUANTA_HOME/bin:\$PATH\""
            } >> "$profile"
            success "Added to $profile"
            info "Run 'source $profile' to update current shell"
        fi
    fi
}

# Main installation flow
main() {
    echo ""
    echo "  ╔═══════════════════════════════════════╗"
    echo "  ║     QuantaLang Installer v1.0.0       ║"
    echo "  ╚═══════════════════════════════════════╝"
    echo ""
    
    detect_platform
    get_latest_version
    download
    verify
    setup_path
    
    echo ""
    success "QuantaLang v$VERSION installed successfully!"
    echo ""
    echo "  To get started, run:"
    echo ""
    echo "    quanta --help"
    echo "    quanta new my-project"
    echo ""
    echo "  Documentation: https://docs.quantalang.org"
    echo ""
}

main
