# Installs the paste-preview Claude Code plugin and its helper on Windows. -Uninstall removes both.
param(
    [switch]$Uninstall,
    [string]$Helper = "",
    [string]$Marketplace = ""
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Repo = if ($env:PASTE_PREVIEW_REPO) { $env:PASTE_PREVIEW_REPO } else { "0010aor/claude-paste-preview" }
$InstallDir = Join-Path $env:LOCALAPPDATA "claude-paste-preview"
$HelperPath = Join-Path $InstallDir "claude-paste-helper.exe"
$Plugin = "paste-preview@paste-preview"
if (-not $Marketplace) { $Marketplace = $Repo }

function Write-Title([string]$Text) { Write-Host ""; Write-Host $Text -ForegroundColor White; Write-Host "" }
function Write-Done([string]$Text, [string]$Detail = "") {
    Write-Host "  $([char]0x2713) " -ForegroundColor Green -NoNewline
    Write-Host $Text -NoNewline
    if ($Detail) { Write-Host " $Detail" -ForegroundColor DarkGray } else { Write-Host "" }
}
function Stop-Install([string]$Text) { Write-Host ""; Write-Host "  $([char]0x2717) $Text" -ForegroundColor Red; Write-Host ""; exit 1 }

if (-not (Get-Command claude -ErrorAction SilentlyContinue)) {
    Stop-Install "Claude Code (claude) is not on PATH. Install it first: https://claude.com/claude-code"
}

if ($Uninstall) {
    Write-Title "Removing paste-preview"
    claude plugin uninstall $Plugin 2>$null | Out-Null
    claude plugin marketplace remove paste-preview 2>$null | Out-Null
    Write-Done "Removed the plugin from Claude Code"
    Remove-Item -Recurse -Force $InstallDir -ErrorAction SilentlyContinue
    Write-Done "Removed the helper" "($InstallDir)"
    Write-Host ""; Write-Host "paste-preview is uninstalled."; Write-Host ""
    exit 0
}

Write-Title "Installing paste-preview for Claude Code"
New-Item -ItemType Directory -Force $InstallDir | Out-Null
if ($Helper) {
    Copy-Item $Helper $HelperPath -Force
    $Detail = "from your local build"
} else {
    $Url = "https://github.com/$Repo/releases/latest/download/claude-paste-helper-windows-x86_64.exe"
    $Download = "$HelperPath.download"
    try {
        Invoke-WebRequest $Url -OutFile $Download -UseBasicParsing
        $Expected = ((Invoke-WebRequest "$Url.sha256" -UseBasicParsing).Content -split '\s+')[0]
    } catch { Stop-Install "could not download the helper from $Url" }
    if ((Get-FileHash $Download -Algorithm SHA256).Hash -ne $Expected.ToUpper()) {
        Remove-Item $Download
        Stop-Install "the downloaded helper failed its checksum; nothing was installed"
    }
    Move-Item $Download $HelperPath -Force
    $Detail = "checksum verified"
}
Write-Done "Installed $(& $HelperPath --version)" "($Detail, $HelperPath)"

claude plugin marketplace add $Marketplace 2>$null | Out-Null
claude plugin install $Plugin | Out-Null
if ($LASTEXITCODE -ne 0) { Stop-Install "Claude Code could not install the plugin" }
Write-Done "Installed the paste-preview plugin in Claude Code"

Write-Host ""; Write-Host "Done." -ForegroundColor White -NoNewline; Write-Host " Restart Claude Code, then paste an image with Ctrl+V."; Write-Host ""
