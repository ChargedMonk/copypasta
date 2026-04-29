## Copypasta (Rust)

Fast, local-only clipboard history + multi-paste for Windows.

### Current features

- **Very small UI**: `Win + Ctrl + Alt + V` opens a minimal picker (Win32 popup + listbox).
- **Paste + recency**: selecting an item sets it as the Windows clipboard, sends `Ctrl + V` to the previously focused app, and **moves the item to the top**.
- **Deduplication**: items are deduped by a BLAKE3 fingerprint (text is normalized; non-text uses canonical formats).
- **Rich clipboard formats (best-effort)**: captures/restores multiple formats when they are backed by global memory (Unicode text, ANSI text, HTML, RTF, DIB/DIBV5 images, file drops).
- **Local persistence + lazy loading**: metadata in SQLite, payloads stored as per-item files; payloads are loaded from disk only when pasted.
- **Tray controls**: notification-area icon with **Pause capture**, **Clear history**, **Exit**.

### Hotkeys

- **Win + Ctrl + Alt + V**: open picker
- **Win + Ctrl + Alt + C**: reserved (capture is currently automatic via clipboard updates)
- **Win + Ctrl + Alt + Q**: dev-only quit hotkey

Hotkeys are configurable in `hotkeys.json` under the per-user app config directory. Copypasta creates this file on first run:

```json
{
  "open_picker": {
    "modifiers": ["win", "ctrl", "alt"],
    "key": "V"
  },
  "capture": {
    "modifiers": ["win", "ctrl", "alt"],
    "key": "C"
  },
  "quit_dev": {
    "modifiers": ["win", "ctrl", "alt"],
    "key": "Q"
  }
}
```

Supported modifier names: `win`, `ctrl`, `alt`, `shift`. Supported keys: single letters/digits and `F1` through `F24`.

### Build & run

```bash
cargo build
cargo run
```

For a release build:

```bash
cargo build --release
```

Release builds hide the console window (see `src/main.rs`).

### Data location

Copypasta stores data under your per-user local app data directory (via `directories::ProjectDirs`):

- **DB**: `copypasta.db`
- **Payload files**: `items/<id>/fmt_<format>.bin`

### Store packaging (MSIX)

This repo doesn’t include a full MSIX pipeline yet, but the app is designed to be store-friendly:

- local-only storage
- no network permissions
- minimal resident footprint

Typical next steps:

- generate MSIX using a Rust packaging tool (e.g. `cargo-packager`) or Windows tooling
- add AppX manifest + icons
- validate capabilities/privacy disclosure

