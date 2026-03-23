#!/usr/bin/env bash
#
# QuantaLang Build Script
#
# Usage:
#   ./build.sh [debug|release|test|clean|install|docs]
#

set -e

# Configuration
PROJECT_NAME="quantalang"
VERSION="1.0.0"

# Directories
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_DIR="$ROOT_DIR/src"
BUILD_DIR="$ROOT_DIR/build"
TARGET_DIR="$ROOT_DIR/target"
DOCS_DIR="$ROOT_DIR/docs"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Logging
info() { echo -e "${BLUE}[INFO]${NC} $1"; }
success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }

# Detect platform
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"
    
    case "$OS" in
        Linux)  PLATFORM="linux" ;;
        Darwin) PLATFORM="macos" ;;
        MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
        *) error "Unsupported OS: $OS"; exit 1 ;;
    esac
    
    case "$ARCH" in
        x86_64|amd64) ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *) error "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    
    TARGET="${ARCH}-${PLATFORM}"
}

# Check dependencies
check_deps() {
    local missing=()
    
    command -v clang &>/dev/null || command -v gcc &>/dev/null || missing+=("C compiler (clang or gcc)")
    command -v llvm-config &>/dev/null || missing+=("LLVM")
    
    if [ ${#missing[@]} -ne 0 ]; then
        error "Missing dependencies:"
        for dep in "${missing[@]}"; do
            echo "  - $dep"
        done
        echo ""
        echo "Install with:"
        case "$PLATFORM" in
            linux)
                echo "  Ubuntu/Debian: sudo apt install clang llvm-dev"
                echo "  Fedora: sudo dnf install clang llvm-devel"
                echo "  Arch: sudo pacman -S clang llvm"
                ;;
            macos)
                echo "  brew install llvm"
                ;;
        esac
        exit 1
    fi
}

# Build in debug mode
build_debug() {
    info "Building in debug mode..."
    mkdir -p "$TARGET_DIR/debug"
    
    # Bootstrap compiler
    if [ ! -f "$TARGET_DIR/bootstrap/quantac" ]; then
        info "Building bootstrap compiler..."
        build_bootstrap
    fi
    
    # Compile with debug flags
    "$TARGET_DIR/bootstrap/quantac" \
        --target "$TARGET" \
        --debug \
        --output "$TARGET_DIR/debug/quanta" \
        "$SRC_DIR/main.quanta"
    
    # Build standard library
    build_stdlib debug
    
    success "Debug build complete: $TARGET_DIR/debug/quanta"
}

# Build in release mode
build_release() {
    info "Building in release mode..."
    mkdir -p "$TARGET_DIR/release"
    
    # Bootstrap compiler
    if [ ! -f "$TARGET_DIR/bootstrap/quantac" ]; then
        info "Building bootstrap compiler..."
        build_bootstrap
    fi
    
    # Compile with optimizations
    "$TARGET_DIR/bootstrap/quantac" \
        --target "$TARGET" \
        --release \
        --opt-level 3 \
        --lto \
        --output "$TARGET_DIR/release/quanta" \
        "$SRC_DIR/main.quanta"
    
    # Build standard library
    build_stdlib release
    
    # Strip binary (optional)
    if command -v strip &>/dev/null && [ "$PLATFORM" != "macos" ]; then
        strip "$TARGET_DIR/release/quanta"
    fi
    
    success "Release build complete: $TARGET_DIR/release/quanta"
    
    # Print binary size
    local size=$(ls -lh "$TARGET_DIR/release/quanta" | awk '{print $5}')
    info "Binary size: $size"
}

# Build bootstrap compiler
build_bootstrap() {
    info "Building bootstrap compiler from C..."
    mkdir -p "$TARGET_DIR/bootstrap"
    
    local CC="${CC:-clang}"
    local CFLAGS="-O2 -Wall"
    local LLVM_FLAGS="$(llvm-config --cflags --ldflags --libs core native)"
    
    $CC $CFLAGS \
        -o "$TARGET_DIR/bootstrap/quantac" \
        "$SRC_DIR/bootstrap/"*.c \
        $LLVM_FLAGS
    
    success "Bootstrap compiler built"
}

# Build standard library
build_stdlib() {
    local mode="${1:-release}"
    info "Building standard library ($mode)..."
    
    local outdir="$TARGET_DIR/$mode/lib"
    mkdir -p "$outdir"
    
    local flags=""
    if [ "$mode" = "release" ]; then
        flags="--release --opt-level 3"
    else
        flags="--debug"
    fi
    
    "$TARGET_DIR/bootstrap/quantac" \
        --lib \
        $flags \
        --output "$outdir/libstd.a" \
        "$SRC_DIR/std/mod.quanta"
    
    success "Standard library built: $outdir/libstd.a"
}

# Run tests
run_tests() {
    info "Running tests..."
    
    local test_type="${1:-all}"
    local target="${TARGET_DIR}/debug/quanta"
    
    if [ ! -f "$target" ]; then
        build_debug
    fi
    
    case "$test_type" in
        unit)
            info "Running unit tests..."
            "$target" test "$SRC_DIR" --unit
            ;;
        integration)
            info "Running integration tests..."
            "$target" test "$ROOT_DIR/tests" --integration
            ;;
        all|*)
            info "Running all tests..."
            "$target" test "$SRC_DIR" --unit
            "$target" test "$ROOT_DIR/tests" --integration
            ;;
    esac
    
    success "All tests passed!"
}

# Generate documentation
build_docs() {
    info "Generating documentation..."
    
    local target="${TARGET_DIR}/debug/quanta"
    
    if [ ! -f "$target" ]; then
        build_debug
    fi
    
    mkdir -p "$DOCS_DIR/api/generated"
    
    "$target" doc "$SRC_DIR" \
        --output "$DOCS_DIR/api/generated" \
        --format html
    
    success "Documentation generated: $DOCS_DIR/api/generated"
}

# Clean build artifacts
clean() {
    info "Cleaning build artifacts..."
    rm -rf "$TARGET_DIR"
    rm -rf "$BUILD_DIR"
    success "Clean complete"
}

# Install to system
install_system() {
    local prefix="${1:-/usr/local}"
    
    info "Installing to $prefix..."
    
    if [ ! -f "$TARGET_DIR/release/quanta" ]; then
        build_release
    fi
    
    # Check permissions
    if [ ! -w "$prefix" ]; then
        error "Cannot write to $prefix (try with sudo)"
        exit 1
    fi
    
    # Install binary
    install -d "$prefix/bin"
    install -m 755 "$TARGET_DIR/release/quanta" "$prefix/bin/"
    
    # Install standard library
    install -d "$prefix/lib/quanta"
    install -m 644 "$TARGET_DIR/release/lib/"* "$prefix/lib/quanta/"
    
    # Install headers/includes
    if [ -d "$SRC_DIR/include" ]; then
        install -d "$prefix/include/quanta"
        cp -r "$SRC_DIR/include/"* "$prefix/include/quanta/"
    fi
    
    success "Installed to $prefix"
    info "Make sure $prefix/bin is in your PATH"
}

# Create distribution package
dist() {
    info "Creating distribution package..."
    
    build_release
    build_docs
    
    local dist_dir="$TARGET_DIR/dist"
    local pkg_name="quanta-${VERSION}-${TARGET}"
    local pkg_dir="$dist_dir/$pkg_name"
    
    rm -rf "$pkg_dir"
    mkdir -p "$pkg_dir"/{bin,lib,doc,examples}
    
    # Copy files
    cp "$TARGET_DIR/release/quanta" "$pkg_dir/bin/"
    cp "$TARGET_DIR/release/lib/"* "$pkg_dir/lib/"
    cp -r "$DOCS_DIR/"* "$pkg_dir/doc/"
    cp -r "$ROOT_DIR/examples/"* "$pkg_dir/examples/"
    cp "$ROOT_DIR/LICENSE-MIT" "$ROOT_DIR/LICENSE-APACHE" "$pkg_dir/"
    cp "$ROOT_DIR/README.md" "$pkg_dir/"
    
    # Create archive
    cd "$dist_dir"
    tar -czf "${pkg_name}.tar.gz" "$pkg_name"
    
    success "Distribution package created: $dist_dir/${pkg_name}.tar.gz"
    
    # Print checksums
    info "Checksums:"
    sha256sum "${pkg_name}.tar.gz"
}

# Print usage
usage() {
    echo "QuantaLang Build System"
    echo ""
    echo "Usage: ./build.sh [COMMAND]"
    echo ""
    echo "Commands:"
    echo "  debug     Build in debug mode (default)"
    echo "  release   Build in release mode with optimizations"
    echo "  test      Run the test suite"
    echo "  docs      Generate documentation"
    echo "  clean     Remove build artifacts"
    echo "  install   Install to system (default: /usr/local)"
    echo "  dist      Create distribution package"
    echo "  help      Show this help message"
    echo ""
    echo "Environment Variables:"
    echo "  CC        C compiler to use (default: clang)"
    echo "  PREFIX    Installation prefix (default: /usr/local)"
}

# Main
main() {
    detect_platform
    
    case "${1:-debug}" in
        debug)
            check_deps
            build_debug
            ;;
        release)
            check_deps
            build_release
            ;;
        test)
            check_deps
            run_tests "${2:-all}"
            ;;
        docs)
            check_deps
            build_docs
            ;;
        clean)
            clean
            ;;
        install)
            install_system "${PREFIX:-/usr/local}"
            ;;
        dist)
            check_deps
            dist
            ;;
        help|--help|-h)
            usage
            ;;
        *)
            error "Unknown command: $1"
            usage
            exit 1
            ;;
    esac
}

main "$@"
