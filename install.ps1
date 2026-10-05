# Installs the paste-preview Claude Code plugin and its helper on Windows. -Uninstall removes both.
param(
    [switch]$Uninstall,
    [string]$Helper = "",
    [string]$Marketplace = ""
)
$ErrorActionPreference = "Stop"

$Repo = if ($env:PASTE_PREVIEW_REPO) { $env:PASTE_PREVIEW_REPO } else { "0010aor/claude-paste-preview" }
$InstallDir = Join-Path $env:LOCALAPPDATA "claude-paste-preview"
$HelperPath = Join-Path $InstallDir "claude-paste-helper.exe"
$Plugin = "paste-preview@paste-preview"
if (-not $Marketplace) { $Marketplace = $Repo }

if (-not (Get-Command claude -ErrorAction SilentlyContinue)) { throw "Claude Code (claude) is not on PATH" }

if ($Uninstall) {
    claude plugin uninstall $Plugin 2>$null | Out-Null
    claude plugin marketplace remove paste-preview 2>$null | Out-Null
    Remove-Item -Recurse -Force $InstallDir -ErrorAction SilentlyContinue
    Write-Output "Removed the plugin and the helper."
    exit 0
}

New-Item -ItemType Directory -Force $InstallDir | Out-Null
if ($Helper) {
    Copy-Item $Helper $HelperPath -Force
} else {
    $Asset = "claude-paste-helper-windows-x86_64.exe"
    $Url = "https://github.com/$Repo/releases/latest/download/$Asset"
    $Download = "$HelperPath.download"
    Invoke-WebRequest $Url -OutFile $Download -UseBasicParsing
    $Expected = ((Invoke-WebRequest "$Url.sha256" -UseBasicParsing).Content -split '\s+')[0]
    $Actual = (Get-FileHash $Download -Algorithm SHA256).Hash
    if ($Actual -ne $Expected.ToUpper()) { Remove-Item $Download; throw "checksum mismatch for the downloaded helper" }
    Move-Item $Download $HelperPath -Force
}
Write-Output "Installed $(& $HelperPath --version) to $HelperPath"

claude plugin marketplace add $Marketplace | Out-Null
claude plugin install $Plugin | Out-Null
Write-Output "Installed the $Plugin Claude Code plugin. Restart Claude Code and paste an image."
