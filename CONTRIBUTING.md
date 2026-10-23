# Contributing

Use stable Rust with the MSVC toolchain on Windows. The Windows SDK resource compiler (`rc.exe`) is required to embed the application icon and version information.

## Build

```powershell
cargo fmt --check
cargo build --release --locked
```

## Source layout

| Module | Responsibility |
| --- | --- |
| `ble` | Windows Bluetooth LE connection, discovery and GATT requests |
| `protocol` | BMAP frames and Bluetooth packet segmentation |
| `model` | Device state, commands, slider drafts and write ordering |
| `worker` | Connection lifecycle and serialized device operations |
| `platform` | Windows single-instance handling and tray message thread |
| `localization` | Language catalogs and preferences |
| `ui` | Rendering, input and responsive layout |

Keep Bluetooth operations off the UI and tray threads. Preserve pending slider edits when older replies arrive.

When reporting a device issue, include the headphone model, firmware, Windows version and steps to reproduce it. Note whether another Bose app is connected.
