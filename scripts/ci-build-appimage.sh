#!/usr/bin/env bash
# CI build: release binaries + AppImage + SHA256SUMS in dist/.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/panteon-ms89/Cargo.toml" | head -1)"
[ -n "$VERSION" ] || { echo "error: cannot read crate version" >&2; exit 1; }

echo "==> release binaries"
cargo build --release --locked --features gui --bins \
    --manifest-path "$ROOT/panteon-ms89/Cargo.toml"

echo "==> AppImage"
cd "$ROOT/packaging/appimage"
rm -rf AppDir appimage-build .bundle.yml ./*.squashfs ./*.AppImage
APPIMAGE_EXTRACT_AND_RUN=1 appimage-builder --recipe AppImageBuilder.yml --skip-tests

mkdir -p "$ROOT/dist"
cp "PanteonConfig-${VERSION}-x86_64.AppImage" "$ROOT/dist/"
cd "$ROOT/dist"
sha256sum "PanteonConfig-${VERSION}-x86_64.AppImage" > SHA256SUMS
echo "==> artifacts"
ls -lh