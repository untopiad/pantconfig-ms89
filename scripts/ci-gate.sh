#!/usr/bin/env bash
# CI gate (also runnable locally): tests, checks, license policy, notices
# determinism and packaging-asset validation. Exits non-zero on any failure.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> test gate (base)"
cargo test --quiet --manifest-path panteon-ms89/Cargo.toml --lib --tests

echo "==> test gate (gui)"
cargo test --quiet --manifest-path panteon-ms89/Cargo.toml --features gui --lib --tests

echo "==> cargo check (bins)"
cargo check --quiet --manifest-path panteon-ms89/Cargo.toml --bins
cargo check --quiet --manifest-path panteon-ms89/Cargo.toml --features gui --bins

echo "==> license policy"
( cd panteon-ms89 && cargo deny --all-features check licenses )

echo "==> third-party notices determinism"
if ! python3 -c "import tomllib" 2>/dev/null; then
    python3 -m pip install --quiet tomli
fi
python3 scripts/gen-notices.py
git diff --exit-code -- panteon-ms89/THIRD-PARTY-NOTICES.md

echo "==> desktop entry / AppStream validation"
desktop-file-validate assets/io.gitlab.p4650.pantconfig.desktop
appstreamcli validate --no-net assets/io.gitlab.p4650.pantconfig.metainfo.xml

echo "==> gate OK"