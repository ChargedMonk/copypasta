## Rip Multi Paste

Fast, local-only clipboard history, search, and multi-paste for Windows.

### Current features

- **Very small UI**: `Win + Ctrl + Alt + V` opens a minimal searchable picker (Win32 popup + listbox).
- **Paste + recency**: selecting an item sets it as the Windows clipboard, sends `Ctrl + V` to the previously focused app, and **moves the item to the top**.
- **Deduplication**: items are deduped by a BLAKE3 fingerprint (text is normalized; non-text uses canonical formats).
- **Rich clipboard formats (best-effort)**: captures/restores multiple formats when they are backed by global memory (Unicode text, ANSI text, HTML, RTF, DIB/DIBV5 images, file drops).
- **Local persistence + lazy loading**: metadata in SQLite, payloads stored as per-item files; payloads are loaded from disk only when pasted. Rip Multi Paste stores more history than Windows clipboard history's small recent-item limit.
- **Tray controls**: notification-area icon with **Pause capture**, **Clear history**, **Exit**.

### Hotkeys

- **Win + Ctrl + Alt + V**: open picker
- **Win + Ctrl + Alt + C**: copy current selection into Windows clipboard, then capture it via clipboard updates
- **Win + Ctrl + Alt + Q**: dev-only quit hotkey

Hotkeys are configurable in `hotkeys.json` under the per-user app config directory. Rip Multi Paste creates this file on first run:

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

### Benchmarks

Rip Multi Paste has two benchmark layers:

- **In-process Criterion benchmarks** for deterministic hot paths:

```bash
cargo bench --bench history --bench picker_search
```

- **Picker-open latency probes** for the user-visible path. In a dev build, pressing the picker
  hotkey logs a `picker latency` event with:
  `hotkey_to_open_us`, `snapshot_us`, `class_registration_us`, `create_window_us`,
  `show_window_us`, `total_us`, and `items`.

Use those logs for Rip Multi Paste's hotkey-to-picker-submitted timing. Compare Windows `Win+V`
with an external workflow measurement, because the Windows clipboard history UI belongs to
the shell and does not expose a stable public timing API.

Current local Criterion baseline:

| Benchmark | Mean-ish range |
| --- | ---: |
| History add 50 unique items | 15.979-18.618 us |
| History bump oldest item in 50 | 6.9091-8.2583 us |
| History remove middle item in 50 | 5.6900-6.8097 us |
| Picker search empty query, 50 short previews | 1.6069-1.9752 us |
| Picker search matching query, 50 short previews | 16.715-20.005 us |
| Picker search matching query, 50 long previews | 17.398-21.752 us |
| Picker search missing query, 50 long previews | 19.189-22.454 us |

### Coverage

Coverage uses `cargo-llvm-cov`:

```bash
cargo install cargo-llvm-cov
cargo llvm-cov --html
cargo llvm-cov report --summary-only
```

Current local baseline after adding the first coverage pass:

- Tests: 33 passed
- Line coverage: 33.74%
- Function coverage: 46.73%
- Region coverage: 36.92%
- HTML report: `target/llvm-cov/html`

### Data location

Rip Multi Paste stores data under your per-user local app data directory (via `directories::ProjectDirs`):

- **DB**: `copypasta.db`
- **Payload files**: `items/<id>/fmt_<format>.bin`

### Store packaging (MSIX)

This repo includes Microsoft Store packaging scaffolding under `packaging/`:

- `packaging/msix/AppxManifest.xml` contains the Partner Center package identity.
- `packaging/msix/Assets/` contains generated MSIX logo assets.
- `packaging/store/` contains Store listing and privacy policy drafts.
- `scripts/package-msix.ps1` builds and stages the MSIX package.

Build a Store package from PowerShell:

```powershell
.\scripts\package-msix.ps1
```

Before submission, host the privacy policy, review the listing draft, run the Windows App Certification Kit, and upload the generated MSIX in Partner Center.

