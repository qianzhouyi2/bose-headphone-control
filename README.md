# Bose Headphone Control

A Windows app for controlling Bose QuietComfort headphones.

[简体中文](README.zh-CN.md) · [Contributing](CONTRIBUTING.md) · [Language packs](docs/languages.md)

![Headphone controls](docs/screenshot.png)

## Features

- Listening modes, noise cancellation, three-band equalizer, immersive audio and wind reduction.
- Paired devices, multipoint connection, LE Audio control, wearing detection and call settings.
- English and Simplified Chinese, with the system language selected by default.
- Tray controls to reopen the window or quit the app.

Tested with **Bose QuietComfort Ultra Headphones (2nd Gen)**. Other models are untested. Available controls depend on the headphones and firmware.

## Headphone firmware

**Bose QuietComfort Ultra Headphones (2nd Gen) requires firmware newer than 10.12.12.**

- **Mobile:** Use the Bose app to check and update the headphone firmware.
- **PC:** In **About headphones**, use **Check updates** to check online while connected over Bluetooth. **USB update** opens [Bose Updater](https://btu.bose.com/) in your browser. Install the official updater when prompted, connect the headphones with a USB data cable, and follow the page to install updates.

## Use

Download the Windows x64 ZIP from [Releases](../../releases/latest), extract it, and run `bose-headphone-control.exe`.

Requires Windows 10/11, a Bluetooth adapter and headphones paired in Windows Settings. Run `bose-headphone-control.exe` to connect.

The Devices page shows connection and playback-source status. **Disconnect** removes the Bluetooth connection without deleting the pairing.

**PC Bluetooth requirement:** The app uses BLE/GATT first and tries RFCOMM if that management connection is unavailable. The Bluetooth adapter and its driver must support the available connection method. If audio playback works but the app cannot connect, also check for a Bluetooth driver update from your PC manufacturer.

The BLE management connection is separate from LE Audio. The app does not require the Windows LE Audio option to be enabled.

Closing the window keeps the app in the tray. Click the tray icon to reopen it, or right-click it to quit. Launching the app again opens the existing window.

Change the display language under **About headphones → Language**. Preferences and logs are stored in `%LOCALAPPDATA%\Bose Headphone Control`.

## Build

Install stable Rust with the MSVC toolchain, Visual Studio C++ Build Tools and the Windows SDK. From the project directory, run:

```powershell
cargo build --release --locked
```

The executable is written to `target/release/bose-headphone-control.exe`. No installer is required. Both display languages are built into the executable.

## Current limitations

The app prefers BLE/GATT and falls back to RFCOMM. Bose QuietComfort Ultra Headphones (2nd Gen) uses RFCOMM channel 2; other models and firmware versions are unverified.

Headphone controls require a Bluetooth management connection, including when audio is playing over USB. Firmware installation over USB uses the official Bose Updater.

## License

[MIT](LICENSE). The project includes protocol compatibility work based on [aaronsb/bosectl](https://github.com/aaronsb/bosectl); the required original license notices are retained.

Acknowledgements: this project builds on the earlier Quiet Rebellion client.

Bose is a trademark of its owner. This is an unofficial project, unaffiliated with Bose Corporation.

The headphone product image belongs to Bose and is excluded from the MIT license. Its source is listed in [LICENSE](LICENSE).
