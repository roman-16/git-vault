<#
.SYNOPSIS
  git-vault installer for Windows.

.DESCRIPTION
  Downloads the latest git-vault release from GitHub Releases, verifies its
  SHA-256 checksum, and installs it into a user directory. No package manager
  required. (winget remains the recommended Windows channel:
  `winget install Roman-16.GitVault`.)

.EXAMPLE
  irm https://raw.githubusercontent.com/roman-16/git-vault/main/scripts/install.ps1 | iex

.EXAMPLE
  # Pin a version or install directory (run the script directly, not piped):
  .\install.ps1 -Version 1.0.0 -InstallDir "C:\tools\git-vault"
#>
[CmdletBinding()]
param(
    [string]$Version = $env:GIT_VAULT_VERSION,
    [string]$InstallDir = $(if ($env:GIT_VAULT_INSTALL_DIR) { $env:GIT_VAULT_INSTALL_DIR } else { "$env:LOCALAPPDATA\Programs\git-vault" })
)

$ErrorActionPreference = 'Stop'
try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch {}

$repo = 'roman-16/git-vault'
$asset = 'git-vault_windows_amd64.exe'

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Warning "git is not on your PATH, and git-vault cannot work without it."
}

$base = if ($Version) {
    "https://github.com/$repo/releases/download/v$($Version.TrimStart('v'))"
} else {
    "https://github.com/$repo/releases/latest/download"
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("git-vault-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp -Force | Out-Null
try {
    Write-Host "Downloading $asset$(if ($Version) { " (v$($Version.TrimStart('v')))" })..."
    Invoke-WebRequest -Uri "$base/$asset" -OutFile "$tmp\$asset" -UseBasicParsing
    Invoke-WebRequest -Uri "$base/checksums.txt" -OutFile "$tmp\checksums.txt" -UseBasicParsing

    $expected = (Select-String -Path "$tmp\checksums.txt" -Pattern "\s$([regex]::Escape($asset))$" |
        Select-Object -First 1).Line -split '\s+' | Select-Object -First 1
    if (-not $expected) { throw "no checksum entry for $asset in checksums.txt" }
    $actual = (Get-FileHash -Algorithm SHA256 -Path "$tmp\$asset").Hash.ToLower()
    if ($expected.ToLower() -ne $actual) { throw "checksum mismatch for $asset (expected $expected, got $actual)" }
    Write-Host "Checksum verified."

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $dest = Join-Path $InstallDir 'git-vault.exe'
    Move-Item -Path "$tmp\$asset" -Destination $dest -Force

    $installed = (& $dest --version 2>$null)
    Write-Host "Installed $installed to $dest" -ForegroundColor Green
}
finally {
    Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $InstallDir) {
    Write-Warning "$InstallDir is not on your PATH, so git will not find ``git vault``. To add it permanently, run:"
    Write-Host "  [Environment]::SetEnvironmentVariable('Path', `"`$([Environment]::GetEnvironmentVariable('Path','User'));$InstallDir`", 'User')"
    $env:Path = "$env:Path;$InstallDir"
}

Write-Host "Start with: git vault init"
