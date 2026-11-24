# PanteonConfig v0.1.1 — AppImage

**Unofficial community project.** PanteonConfig is an independent open-source
utility that makes the PANTEON MS89 mouse configurable on Linux. It is not
affiliated with, endorsed by, or supported by the PANTEON brand or manufacturer;
all trademarks belong to their owners. The software is provided **"AS IS"**
under the MIT license — see `LICENSE` and the README disclaimer.

## What's inside

- `panteon-gui` — desktop configurator (DPI slots, colors, lighting, polling, profiles).
- `panteon-ms89` — the same functionality from the CLI, including `check-access`.
- `usr/share/panteon-ms89/99-panteon-ms89.rules` — the udev rule for non-root access.
- `LICENSE` and `THIRD-PARTY-NOTICES.md` (dependency and bundled-font licenses).

## Requirements

- x86_64 Linux with **glibc ≥ 2.35** (Ubuntu 22.04+, Fedora 36+, Debian 12+).
- An X11 or Wayland desktop session.
- No FUSE required: run the AppImage directly, or with
  `APPIMAGE_EXTRACT_AND_RUN=1` on systems without FUSE.

## Install (3 steps)

1. Download `PanteonConfig-0.1.1-x86_64.AppImage` and `SHA256SUMS` from this
   release's assets.

2. Verify and make executable:

   ```bash
   sha256sum -c SHA256SUMS
   chmod +x PanteonConfig-0.1.1-x86_64.AppImage
   ```

3. Install the udev rule **once** — without it the mouse is not visible
   (hidraw and the USB-reset node are root-owned by default; the `input` group
   does not cover USB reset):

   ```bash
   curl -fsSL https://gitlab.com/p4650/pantconfig/-/raw/v0.1.1/panteon-ms89/udev/99-panteon-ms89.rules \
     | sudo tee /etc/udev/rules.d/99-panteon-ms89.rules
   sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=hidraw --subsystem-match=usb
   ```

   Offline alternative (the rule is bundled in the image): run
   `./PanteonConfig-0.1.1-x86_64.AppImage check-access` while the rule is
   missing/outdated — it prints a ready-to-run heredoc; the GUI modal has
   «Скопировать команду».

## Run

```bash
./PanteonConfig-0.1.1-x86_64.AppImage                # GUI (no arguments)
./PanteonConfig-0.1.1-x86_64.AppImage --help         # CLI
./PanteonConfig-0.1.1-x86_64.AppImage check-access   # read-only access diagnostics
```

No arguments launches the GUI; any arguments are passed to the CLI. No
installation or extraction is required. State (ledger, profiles) lives in
`~/.local/state/panteon-ms89`.

## Verify access

`check-access` exits **0** when the udev rule is installed and both device nodes
open, **1** otherwise (with the exact reason and the install command). In the GUI
an incomplete setup shows the status-bar chip «Нет доступа к устройству».

## Not in this release

deb/rpm/AUR/Flatpak/Flathub are **not** published yet; their configs remain in
the repository as a dormant asset and can be built locally with
`scripts/build-packages.sh`.