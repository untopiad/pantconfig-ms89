#!/usr/bin/env bash
# Build all release packages for PanteonConfig (version from panteon-ms89/Cargo.toml).
#
# Usage:
#   scripts/build-packages.sh                # build everything
#   scripts/build-packages.sh deb rpm        # only selected formats
#   scripts/build-packages.sh --help
#
# Outputs are copied to dist/ (override with OUT_DIR=/path).
# Requirements are checked per requested format; nothing is silently skipped.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$REPO_ROOT/dist}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_ROOT/panteon-ms89/Cargo.toml" | head -1)"
FORMATS=(deb rpm appimage flatpak)

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
info() { printf '\n==> %s\n' "$*"; }

usage() {
    cat <<'EOF'
Build PanteonConfig packages locally.

Usage: scripts/build-packages.sh [FORMAT...]

Formats: deb rpm appimage flatpak (default: all four)

Environment:
  OUT_DIR   output directory (default: <repo>/dist)
EOF
}

require_tool() {
    command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1 ($2)"
}

check_tools() {
    local f
    for f in "$@"; do
        case "$f" in
            deb)
                require_tool cargo "install Rust toolchain"
                require_tool cargo-deb "cargo install cargo-deb"
                require_tool dpkg-deb "apt install dpkg-dev"
                ;;
            rpm)
                require_tool cargo "install Rust toolchain"
                require_tool cargo-generate-rpm "cargo install cargo-generate-rpm"
                ;;
            appimage)
                require_tool cargo "install Rust toolchain"
                require_tool appimage-builder "pip install appimage-builder"
                ;;
            flatpak)
                require_tool cargo "install Rust toolchain"
                require_tool flatpak-builder "install flatpak-builder"
                require_tool flatpak "install flatpak"
                ;;
            *)
                die "unknown format: $f"
                ;;
        esac
    done
    require_tool desktop-file-validate "apt install desktop-file-utils"
    require_tool appstreamcli "apt install appstream"
}

validate_assets() {
    info "validating desktop entry and AppStream metadata"
    desktop-file-validate "$REPO_ROOT/assets/io.gitlab.p4650.pantconfig.desktop"
    appstreamcli validate --no-net "$REPO_ROOT/assets/io.gitlab.p4650.pantconfig.metainfo.xml"
}

build_binaries() {
    info "building release binaries (--features gui)"
    cargo build --release --locked --features gui --bins \
        --manifest-path "$REPO_ROOT/panteon-ms89/Cargo.toml"
}

build_deb() {
    info "building .deb (cargo-deb)"
    ( cd "$REPO_ROOT/panteon-ms89" && cargo deb --features gui )
    cp "$REPO_ROOT"/panteon-ms89/target/debian/panteon-ms89_"$VERSION"-1_amd64.deb "$OUT_DIR"/
}

build_rpm() {
    info "building .rpm (cargo-generate-rpm)"
    # cargo-generate-rpm has no --features flag; binaries are built above.
    ( cd "$REPO_ROOT/panteon-ms89" && cargo generate-rpm )
    cp "$REPO_ROOT"/panteon-ms89/target/generate-rpm/panteon-ms89-"$VERSION"-1.x86_64.rpm "$OUT_DIR"/
}

build_appimage() {
    info "building AppImage (appimage-builder)"
    # appimage-builder caches an apt venv under appimage-build/; stale caches
    # produce InvalidVersion failures across runs, so start clean.
    rm -rf "$REPO_ROOT/packaging/appimage/AppDir" \
           "$REPO_ROOT/packaging/appimage/appimage-build" \
           "$REPO_ROOT/packaging/appimage/.bundle.yml" \
           "$REPO_ROOT"/packaging/appimage/*.squashfs \
           "$REPO_ROOT"/packaging/appimage/*.AppImage
    ( cd "$REPO_ROOT/packaging/appimage" \
        && appimage-builder --recipe AppImageBuilder.yml --skip-tests )
    cp "$REPO_ROOT"/packaging/appimage/PanteonConfig-"$VERSION"-x86_64.AppImage "$OUT_DIR"/
}

flatpak_finish_args=(
    --command=panteon-gui
    --device=all
    --socket=wayland
    --socket=fallback-x11
    --share=ipc
    --talk-name=org.freedesktop.portal.Desktop
)

build_flatpak() {
    info "building Flatpak bundle"
    local build_dir="$REPO_ROOT/build-dir"
    local repo_dir="$REPO_ROOT/flatpak-repo"
    rm -rf "$build_dir" "$repo_dir"

    if ! flatpak-builder --user --force-clean --disable-rofiles-fuse \
            --repo="$repo_dir" "$build_dir" \
            "$REPO_ROOT/packaging/flatpak/io.gitlab.p4650.pantconfig.yml"; then
        # flatpak-builder >= 1.4 calls `appstreamcli compose`, which needs the
        # AppStream 1.x compose addon (/usr/libexec/appstreamcli-compose). On
        # hosts without it, finish and export the already-built tree manually.
        [ -d "$build_dir/files" ] || die "flatpak-builder failed before producing build-dir"
        info "appstream compose unavailable; finalizing build-dir manually"
        flatpak build-finish "${flatpak_finish_args[@]}" "$build_dir"
        flatpak build-export --arch=x86_64 "$repo_dir" "$build_dir"
    fi

    flatpak build-bundle "$repo_dir" "$OUT_DIR/panteon-ms89-$VERSION.flatpak" \
        io.gitlab.p4650.pantconfig
}

main() {
    if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
        usage
        exit 0
    fi

    local selected=("$@")
    [ ${#selected[@]} -eq 0 ] && selected=("${FORMATS[@]}")

    local f
    for f in "${selected[@]}"; do
        case " ${FORMATS[*]} " in
            *" $f "*) ;;
            *) die "unknown format: $f (expected: ${FORMATS[*]})" ;;
        esac
    done

    check_tools "${selected[@]}"
    mkdir -p "$OUT_DIR"

    validate_assets
    build_binaries

    for f in "${selected[@]}"; do
        "build_$f"
    done

    info "done; artifacts in $OUT_DIR"
    ls -lh "$OUT_DIR"
}

main "$@"