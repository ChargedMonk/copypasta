# Microsoft Store Packaging

Rip Multi Paste is prepared for Microsoft Store distribution as a packaged Win32 desktop app using MSIX.

## Partner Center Identity

- Package identity name: `ChargedMonk.RipMultiPaste`
- Publisher: `CN=E8489628-BB4A-4C36-9375-3F37731A69DF`
- Publisher display name: `ChargedMonk`
- Package family name: `ChargedMonk.RipMultiPaste_2ntzwjd1sck7m`
- Store ID: `9P479B0MZTZ8`

The package SID is intentionally not written into the manifest. Windows derives it from the package identity.

## Build An MSIX

Requirements:

- Windows 10 or 11
- Rust stable toolchain
- Windows SDK with `makeappx.exe`

From PowerShell:

```powershell
.\scripts\package-msix.ps1
```

The script builds `target\release\copypasta.exe`, stages it with `packaging\msix\AppxManifest.xml` and `packaging\msix\Assets`, then writes:

```text
target\msix\RipMultiPaste_1.0.1.0_x64.msix
```

The Store re-signs submitted MSIX packages. Use the script's `-PfxPath` option only when you need a locally signed package for sideload testing.

## Startup Behavior

The manifest declares a `windows.startupTask` extension with `Enabled="false"`. This keeps startup user-controlled for Store policy friendliness. Users can enable Rip Multi Paste from Windows Startup Apps settings after installation.

## Before Submission

1. Create and host the privacy policy from `packaging\store\privacy-policy.md`.
2. Review the Store listing draft in `packaging\store\listing.md`.
3. Verify screenshots in `packaging\store\screenshots`.
4. Build the MSIX with `.\scripts\package-msix.ps1`.
5. Run the Windows App Certification Kit against the package.
6. Upload the package and listing assets in Partner Center.
