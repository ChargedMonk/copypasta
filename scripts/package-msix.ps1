param(
    [string]$Configuration = "release",
    [string]$Platform = "x64",
    [string]$PfxPath = "",
    [string]$PfxPassword = ""
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$PackageVersion = "1.0.0.0"
$PackageName = "RipMultiPaste_$PackageVersion`_$Platform"
$StageDir = Join-Path $RepoRoot "target\msix\stage"
$OutputDir = Join-Path $RepoRoot "target\msix"
$PackagePath = Join-Path $OutputDir "$PackageName.msix"
$ManifestPath = Join-Path $RepoRoot "packaging\msix\AppxManifest.xml"
$AssetsDir = Join-Path $RepoRoot "packaging\msix\Assets"

function Get-WindowsKitTool {
    param([string]$ToolName)

    $KitRoot = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
    if (-not (Test-Path $KitRoot)) {
        throw "Windows SDK not found at $KitRoot. Install the Windows SDK to get $ToolName."
    }

    $Tool = Get-ChildItem -Path $KitRoot -Recurse -Filter $ToolName |
        Where-Object { $_.FullName -match "\\x64\\$ToolName$" } |
        Sort-Object FullName -Descending |
        Select-Object -First 1

    if (-not $Tool) {
        throw "$ToolName not found under $KitRoot."
    }

    return $Tool.FullName
}

Push-Location $RepoRoot
try {
    cargo build --release

    if (Test-Path $StageDir) {
        Remove-Item $StageDir -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $StageDir | Out-Null
    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

    Copy-Item (Join-Path $RepoRoot "target\release\copypasta.exe") (Join-Path $StageDir "copypasta.exe")
    Copy-Item $ManifestPath (Join-Path $StageDir "AppxManifest.xml")
    Copy-Item $AssetsDir (Join-Path $StageDir "Assets") -Recurse

    $MakeAppx = Get-WindowsKitTool "makeappx.exe"
    & $MakeAppx pack /d $StageDir /p $PackagePath /overwrite

    if ($PfxPath) {
        $SignTool = Get-WindowsKitTool "signtool.exe"
        if ($PfxPassword) {
            & $SignTool sign /fd SHA256 /a /f $PfxPath /p $PfxPassword $PackagePath
        } else {
            & $SignTool sign /fd SHA256 /a /f $PfxPath $PackagePath
        }
    } else {
        Write-Host "Created unsigned MSIX: $PackagePath"
        Write-Host "Microsoft Store submissions are re-signed by the Store. Sign locally only if you need sideload/install testing."
    }
}
finally {
    Pop-Location
}
