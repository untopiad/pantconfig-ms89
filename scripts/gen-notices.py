#!/usr/bin/env python3
"""Generate a deterministic THIRD-PARTY-NOTICES.md for the panteon-ms89 crate.

The script reads every package recorded in Cargo.lock, resolves its license
expression from the local cargo registry ($CARGO_HOME/registry/src), and emits
a sorted inventory. It also embeds the full license texts of the fonts bundled
by `epaint_default_fonts` (OFL-1.1, Ubuntu-font-1.0, plus the MIT notices for
the other bundled font files).

Output is byte-for-byte reproducible: no timestamps, no absolute paths, entries
sorted by (name, version). Re-run after any Cargo.lock change:

    python3 scripts/gen-notices.py
"""

from __future__ import annotations

import os
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 (e.g. ubuntu:22.04)
    import tomli as tomllib

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT / "panteon-ms89"
LOCK = CRATE / "Cargo.lock"
OUT = CRATE / "THIRD-PARTY-NOTICES.md"
PROJECT_NAME = "panteon-ms89"
PROJECT_LICENSE = "MIT"


def registry_src_dirs() -> list[Path]:
    """Locate extracted cargo registry source directories."""
    home = Path.home()
    roots: list[Path] = []
    cargo_home = os.environ.get("CARGO_HOME")
    if cargo_home:
        roots.append(Path(cargo_home) / "registry" / "src")
    xdg_data = os.environ.get("XDG_DATA_HOME")
    if xdg_data:
        roots.append(Path(xdg_data) / "cargo" / "registry" / "src")
    roots.append(home / ".cargo" / "registry" / "src")
    roots.append(home / ".local" / "share" / "cargo" / "registry" / "src")

    dirs: list[Path] = []
    for root in roots:
        if root.is_dir():
            dirs.extend(sorted(p for p in root.iterdir() if p.is_dir()))
    # De-duplicate while preserving order.
    seen: set[Path] = set()
    unique: list[Path] = []
    for d in dirs:
        resolved = d.resolve()
        if resolved not in seen:
            seen.add(resolved)
            unique.append(d)
    return unique


def read_package_manifest(name: str, version: str, src_dirs: list[Path]) -> dict:
    for src in src_dirs:
        manifest = src / f"{name}-{version}" / "Cargo.toml"
        if manifest.is_file():
            with manifest.open("rb") as fh:
                return tomllib.load(fh)
    return {}


def license_expression(name: str, version: str, src_dirs: list[Path]) -> str:
    if name == PROJECT_NAME:
        return PROJECT_LICENSE
    manifest = read_package_manifest(name, version, src_dirs)
    package = manifest.get("package", {})
    expression = package.get("license")
    if not expression:
        expression = manifest.get("workspace", {}).get("package", {}).get("license")
    if not expression:
        expression = "UNKNOWN (no SPDX license field)"
    return " ".join(expression.split())


def find_font_dir(src_dirs: list[Path]) -> Path:
    for src in src_dirs:
        candidates = sorted(src.glob("epaint_default_fonts-*"))
        if candidates:
            return candidates[-1]
    raise SystemExit(
        "error: epaint_default_fonts not found in the cargo registry.\n"
        "       Run `cargo fetch` (with --features gui) first."
    )


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8").replace("\r\n", "\n").rstrip() + "\n"


def main() -> int:
    if not LOCK.is_file():
        raise SystemExit(f"error: {LOCK} not found")
    with LOCK.open("rb") as fh:
        lock = tomllib.load(fh)

    src_dirs = registry_src_dirs()
    if not src_dirs:
        raise SystemExit(
            "error: no cargo registry sources found. Run `cargo fetch` first."
        )

    rows = []
    for package in lock.get("package", []):
        name = package["name"]
        version = package["version"]
        rows.append((name, version, license_expression(name, version, src_dirs)))
    rows.sort(key=lambda row: (row[0].lower(), row[0], row[1]))

    missing = [row for row in rows if row[2].startswith("UNKNOWN")]
    if missing:
        for name, version, expr in missing:
            print(f"error: {name} {version}: {expr}", file=sys.stderr)
        return 1

    third_party = sum(1 for row in rows if row[0] != PROJECT_NAME)
    font_dir = find_font_dir(src_dirs)
    fonts = font_dir / "fonts"

    out: list[str] = []
    out.append("# Third-Party Notices")
    out.append("")
    out.append(
        f"This project (`{PROJECT_NAME}`, PanteonConfig) is distributed under the "
        f"{PROJECT_LICENSE} license; see `LICENSE`."
    )
    out.append("")
    out.append(
        "This file aggregates the licenses of every package recorded in "
        f"`Cargo.lock` ({len(rows)} entries: {third_party} third-party crates plus "
        "this project). It is generated deterministically by "
        "`scripts/gen-notices.py` and must not be edited by hand."
    )
    out.append("")
    out.append("## Crate inventory")
    out.append("")
    out.append("| Crate | Version | License |")
    out.append("| --- | --- | --- |")
    for name, version, expression in rows:
        out.append(f"| `{name}` | {version} | {expression} |")
    out.append("")
    out.append("## Fonts bundled by `epaint_default_fonts`")
    out.append("")
    out.append(
        "`epaint_default_fonts` embeds font files and requires the following "
        "license texts to be redistributed with any binary that includes them. "
        "The texts below are reproduced from the crate's `fonts/` directory."
    )
    out.append("")
    out.append("### SIL Open Font License 1.1 (OFL-1.1)")
    out.append("")
    out.append("Applies to `NotoEmoji-Regular.ttf`.")
    out.append("")
    out.append("```text")
    out.append(read_text(fonts / "OFL.txt").rstrip("\n"))
    out.append("```")
    out.append("")
    out.append("### Ubuntu Font Licence 1.0 (Ubuntu-font-1.0)")
    out.append("")
    out.append("Applies to `Ubuntu-Light.ttf`.")
    out.append("")
    out.append("```text")
    out.append(read_text(fonts / "UFL.txt").rstrip("\n"))
    out.append("```")
    out.append("")
    out.append("### Additional bundled font notices (MIT)")
    out.append("")
    out.append("Applies to `Hack-Regular.ttf`.")
    out.append("")
    out.append("```text")
    out.append(read_text(fonts / "Hack-Regular.txt").rstrip("\n"))
    out.append("```")
    out.append("")
    out.append("Applies to `emoji-icon-font.ttf`.")
    out.append("")
    out.append("```text")
    out.append(read_text(fonts / "emoji-icon-font-mit-license.txt").rstrip("\n"))
    out.append("```")
    out.append("")
    out.append("## Attribution")
    out.append("")
    out.append(
        "Copyright and license notices are reproduced as shipped by the "
        "respective upstream crates in the Cargo registry. PanteonConfig is an "
        "unofficial community project and is not affiliated with, endorsed by, "
        "or supported by the PANTEON brand or manufacturer."
    )
    out.append("")

    OUT.write_text("\n".join(out), encoding="utf-8")
    print(
        f"wrote {OUT.relative_to(ROOT)}: {len(rows)} lock entries "
        f"({third_party} third-party)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())