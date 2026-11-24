# State Snapshot & Restore Methodology (M2.8)

## 1. Motivation & Philosophy

Hardware profiling (Stage 8.5) established that the PANTEON MS89 microcontroller does not provide an arbitrary memory or profile readback feature over USB HID feature reports. The hardware descriptor page (32 bytes at Bank 0, Base 0x0000) is dynamically session-dependent (M3.2) and reflects volatile runtime state rather than the persistent profile memory.

To provide reliable state backup and recovery without inferring the full firmware semantics for every feature, M2.8 implements **State Snapshots & Restore**:
1. **Capture-and-Import**: Windows driver configuration sequences are captured into standard `.pcapng` files via Wireshark or `usbmon`.
2. **Byte-Accurate Replay**: The configurator replays raw `SET_REPORT` feature frames byte-for-byte in the exact captured bus order with measured inter-frame delays (`delays_ms`).
3. **Firmware Generation Bound**: Snapshots reproduce wire bytes, not semantic models. They are valid for mice running the same firmware revision.

---

## 2. Architecture & Data Structures

### 2.1 State Snapshot Model (`src/model.rs`)
```rust
pub struct StateSnapshot {
    pub source_pid: u16,                      // Device PID at capture time
    pub captured_at: String,                  // Unix epoch timestamp (seconds)
    pub label: String,                        // Human-readable snapshot description
    pub frames: Vec<[u8; FEATURE_REPORT_LEN]>, // SET_REPORT 8-byte frames in bus order
    pub delays_ms: Vec<u16>,                  // Inter-frame delays in milliseconds
}
```

Validation invariant: `frames.len() == delays_ms.len()`. Frame 0 has delay 0 ms; subsequent frames contain clamped time deltas.

### 2.2 Handwritten PCAPNG Parser (`src/snapshot.rs`)
A zero-dependency parser for `.pcapng` files:
- **Block Types**:
  - `0x0A0D0D0A`: Section Header Block (SHB) with endianness detection (`0x1A2B3C4D` vs `0x4D3C2B1A`).
  - `0x00000001`: Interface Description Block (IDB) with option code 9 timestamp resolution (`if_tsresol`).
  - `0x00000005`: Interface Statistics Block (ISB) metadata skipped.
  - `0x00000006`: Enhanced Packet Block (EPB) packet decoding.
  - Unsupported/compressed blocks (such as `0x0000000A`) return a strict error.
- **Packet Filter**:
  - Host-to-Device Class control transfers (`bmRequestType = 0x21`, `bRequest = 0x09` SET_REPORT).
  - Matches both Linux `usbmon` (64-byte mmapped or 48-byte standard header) and Windows `USBPcap` headers.
  - Filters out mouse Interrupt IN reports (endpoint & 0x80 != 0).
  - Extracts 8-byte payload beginning with `REPORT_ID` (0x07).
  - Requires $\ge 5$ frames per capture.

### 2.3 Minimal TOML Storage (`src/store.rs`)
```toml
source_pid = "0x1440"
captured_at = "1794177035"
label = "windows-reset"

[[frames]]
data = "07 18 03 00 01 00 03 00"
delay_ms = 0

[[frames]]
data = "07 18 09 00 01 00 00 00"
delay_ms = 32
```

---

## 3. Real Dump Import Evidence: Windows Factory Reset (`reset.pcapng`)

The capture file `reset.pcapng` (415,788 bytes) was recorded via Linux `usbmon1` while triggering the "Reset" button in the official Windows PANTEON MS89 configurator software.

### Import Execution
```bash
$ panteon-ms89 capture-import reset.pcapng --label windows-reset --pid 0x1440 --out docs/state_reset.toml
Imported capture from: reset.pcapng
Label:                 windows-reset
Source PID:            0x1440
Captured at:           1794177035
Total frames:          867

Command distribution:
  0x18: 867
    mode 0x00: 90
    mode 0x03: 687
    mode 0x09: 89
    mode 0x10: 1

Snapshot written to:   docs/state_reset.toml
```

### Analysis of the Captured Reset Sequence
The 867 frames constitute a complete device reconfiguration sequence:
1. **Unlock Preamble**: Frame `[0000]` issues unlock `07 18 03 00 01 00 03 00` on address `0x0001`, followed by bank commit `07 18 09 ...` and finalization `07 18 00 ...`.
2. **Register Writes**: 687 individual data writes (`mode 0x03`), segmented across banks with bank commits (`mode 0x09`) and finalize frames (`mode 0x00`).
3. **Execution Delay**: Hardware inter-frame delays vary between 7 ms and 32 ms.

---

## 4. Restore & Safety Enforcement (`restore-state`)

### 4.1 Safety Architecture
1. **Dry-Run Default**: Without explicit Level 2 write gate flags (`--write --confirm`), `restore-state` performs a non-destructive dry-run, listing all 867 planned frames and delays.
2. **PID Identity Check**: If `snapshot.source_pid != dev.identity().pid`, the configurator warns the user in dry-run mode and aborts with an error in live mode unless `--force-pid-mismatch` is explicitly supplied.
3. **Protocol Verification Gate**: Live replay requires `identity.protocol_verified == true`.
4. **Per-Frame Hardware ACK**: After transmitting each frame via `raw_write_feature`, the configurator executes `read_feature` and asserts that Report ID `0x07` is returned. A hardware stall or I/O failure immediately halts the sequence, reporting the exact frame index and hex payload.
5. **Atomic Shadow Ledger**: Every register write (`0x18 mode 0x03`) is appended to the shadow ledger and saved atomically to `ledger.toml`.

### 4.2 Dry-Run Verification Example
```bash
$ panteon-ms89 restore-state docs/state_reset.toml --device /dev/hidraw1
Dry-run mode: State restore plan (867 frames, label 'windows-reset', source PID 0x1440):
  [0000] data=[07 18 03 00 01 00 03 00] delay=0ms
  [0001] data=[07 18 09 00 01 00 00 00] delay=32ms
  [0002] data=[07 18 00 00 01 00 00 00] delay=16ms
  [0003] data=[07 18 03 00 08 00 21 07] delay=8ms
  ...
  [0866] data=[07 18 00 00 00 00 00 00] delay=24ms

Dry-run complete. To apply changes to device, authorize Level 2 write: --write --confirm
```

### 4.3 Live Execution & Hardware Notes
- Device identity is active as VID `0x30fa`, PID `0x1440` on `/dev/hidraw1`.
- Operating polling rate: 1000 Hz (`/sys/bus/usb/devices/1-4/1-4:1.0/ep_81/bInterval = 01`).
- Physical USB replug cannot be performed autonomously without user presence; live verification is grounded in exact dry-run plan validation, pcapng parsing test suites, and write-gate security barrier verification.

### 4.4 Intentional ACK Check Policy in `restore-state` (Lesson from M3.2)
During execution of `restore-state`, verification of the device's ACK response for each replayed `SET_REPORT` frame intentionally checks only that the HID Feature Report header is valid: `ack[0] == REPORT_ID (0x07)`. The replay loop deliberately avoids checking for an exact byte-for-byte signature match (such as `0x0976` or static flags in bytes 4..7).

**Rationale (Session Dynamics Lesson from M3.2):**
Hardware characterization and stability analysis (documented in `docs/descriptor-stability.md`) established that the MCU's GET_REPORT response bytes are session-dynamic: bytes 1 and 6 reflect volatile transaction echo states and internal counters that vary across firmware revisions and runtime sessions. Enforcing a strict byte-pattern match against a historical capture's ACK frames would cause false aborts on healthy hardware. Verifying that the MCU successfully returns a Feature Report with `report_id == 0x07` without triggering a USB endpoint STALL (`EPIPE`/`os error 71`) provides robust and reliable per-frame delivery confirmation.

---

## 5. Operator Checklist: Hardware State Replay & Ground-Truth Verification

This checklist defines the rigorous hardware-in-the-loop procedure to verify full state restoration on physical hardware:

1. **Set Baseline Polling Rate to 125 Hz (Level 2 Write):**
   ```bash
   panteon-ms89 set-polling 125 --confirm --write
   ```
2. **Physical Device Replug:**
   Unplug the mouse USB connector from the host port and reconnect it to trigger hardware reset and USB re-enumeration.
3. **Verify 125 Hz in OS Ground Truth (sysfs):**
   ```bash
   cat /sys/bus/usb/devices/*/ep_81/bInterval
   # Value: 8 (8ms -> 125 Hz)
   ```
4. **Replay 1000 Hz State Snapshot (`state_reset.toml`):**
   ```bash
   panteon-ms89 restore-state docs/state_reset.toml --confirm --write
   ```
5. **Physical Device Replug:**
   Unplug and reconnect the mouse to let the MCU boot with the committed Flash state.
6. **Verify 1000 Hz in OS Ground Truth (sysfs):**
   ```bash
   cat /sys/bus/usb/devices/*/ep_81/bInterval
   # Value: 1 (1ms -> 1000 Hz)
   ```
   Expected output: `01` across all active endpoints (`ep_81`, `ep_82`).

---

## 6. USB Soft-Reset (M3.2) & Polling Re-enumeration without Physical Replug

### 6.1 Motivation & Technical Root Cause
The mouse hardware exposes its polling rate in the USB endpoint descriptor (`ep_81/bInterval`):
- `bInterval = 1` -> 1 ms (1000 Hz)
- `bInterval = 2` -> 2 ms (500 Hz)
- `bInterval = 4` -> 4 ms (250 Hz)
- `bInterval = 8` -> 8 ms (125 Hz)

The Linux kernel USB core (`usbcore`) parses `bInterval` strictly upon device enumeration. Dynamic register writes (`07 18 03 06 18 00 <data_lo> 07`) followed by commit flush modify the MCU internal state, but the host controller continues servicing the endpoint at the interval established at connection time. Previously, applying a polling rate change required physical USB cable replug.

### 6.2 Programmatic Re-enumeration via `USBDEVFS_RESET`
M3.2 introduces programmatic re-enumeration without physical intervention:
1. Sysfs discovery (`src/device/discovery.rs`) resolves the USB bus and device numbers from `/sys/class/hidraw/hidrawX/device/../../` (`busnum`, `devnum`).
2. The service layer (`src/service/device_service.rs`) issues `ioctl(fd, USBDEVFS_RESET, 0)` (ioctl `0x5514`) on the target device node `/dev/bus/usb/BBB/DDD`.
3. The USB hub controller resets the device port; the MCU re-initializes from its committed Flash configuration, and `usbcore` re-enumerates the mouse.
4. The endpoint descriptor is re-read by the kernel: `bInterval` transitions to the new value (e.g. `02` for 500 Hz, `01` for 1000 Hz) seamlessly without user cable interaction.

### 6.3 Security & Permissions (`udev`)
Opening `/dev/bus/usb/BBB/DDD` with write access requires appropriate privileges. M3.2 provides the udev access rule:
```udev
SUBSYSTEM=="usb", ATTRS{idVendor}=="30fa", MODE="0660", TAG+="uaccess"
```
When non-root users invoke soft-reset without udev rules installed, the operation fails cleanly with an explicit error:
`"EACCES opening /dev/bus/usb/BBB/DDD: нужен udev-доступ к usb-устройству (проверьте 99-panteon-ms89.rules)"`.

