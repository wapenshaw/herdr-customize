# Herdr's [[build]] step on Windows. Runs in the Windows PowerShell 5.1 that
# ships with Windows, so installing needs no extra tools.
#
# Downloads the release binary for this checkout's version, checks it against
# the release's SHA256SUMS, and installs it where the manifest runs it.
# Anything missing or wrong (no release for this version, no network, a
# checksum mismatch) falls back to building from source with cargo, which
# needs Rust and the Visual Studio Build Tools.
#
# HERDR_CUSTOMIZE_BUILD_FROM_SOURCE=1 skips the download.
# HERDR_CUSTOMIZE_RELEASE_BASE points the download somewhere else (tests).

$ErrorActionPreference = 'Stop'

$Repo = 'wapenshaw/herdr-customize'
$Root = Split-Path -Parent $PSScriptRoot
$Out = Join-Path $Root 'target\release\herdr-customize.exe'
$Base = if ($env:HERDR_CUSTOMIZE_RELEASE_BASE) { $env:HERDR_CUSTOMIZE_RELEASE_BASE } else { "https://github.com/$Repo/releases/download" }
# ARM64 Windows runs the x64 build under emulation.
$Asset = 'herdr-customize-x86_64-pc-windows-msvc.exe'
$script:Tmp = $null

# Windows cannot overwrite a running executable but can rename it, so an
# open popup does not block an update. The old copy is removed next time.
function Move-OldBinary {
    $old = "$Out.old"
    if (Test-Path $old) { Remove-Item -Force $old -ErrorAction SilentlyContinue }
    if (Test-Path $Out) { Move-Item -Force $Out $old }
}

function Build-FromSource([string]$Reason) {
    [Console]::Error.WriteLine("herdr-customize: $Reason; building from source.")
    if ($script:Tmp -and (Test-Path $script:Tmp)) { Remove-Item -Recurse -Force $script:Tmp -ErrorAction SilentlyContinue }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        [Console]::Error.WriteLine("herdr-customize: cargo was not found. Install Rust from https://rustup.rs with the Visual Studio Build Tools, then run: herdr plugin install $Repo")
        exit 1
    }
    try { Move-OldBinary } catch {}
    Push-Location $Root
    try {
        & cargo build --release --locked
        exit $LASTEXITCODE
    } finally {
        Pop-Location
    }
}

if ($env:HERDR_CUSTOMIZE_BUILD_FROM_SOURCE) { Build-FromSource 'HERDR_CUSTOMIZE_BUILD_FROM_SOURCE is set' }

$version = $null
foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
    if ($line -match '^version = "(.+)"$') { $version = $Matches[1]; break }
}
if (-not $version) { Build-FromSource 'Cargo.toml has no version' }

$arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($arch -ne 'AMD64' -and $arch -ne 'ARM64') { Build-FromSource "no release binary for $arch" }

# Windows PowerShell can default to TLS 1.0, which GitHub refuses.
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch {}

$script:Tmp = Join-Path ([IO.Path]::GetTempPath()) ("herdr-customize-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $script:Tmp | Out-Null
$bin = Join-Path $script:Tmp $Asset
$sums = Join-Path $script:Tmp 'SHA256SUMS'
$url = "$Base/v$version"
try {
    Invoke-WebRequest -Uri "$url/$Asset" -OutFile $bin -UseBasicParsing
    Invoke-WebRequest -Uri "$url/SHA256SUMS" -OutFile $sums -UseBasicParsing
} catch {
    Build-FromSource "no $Asset for v$version ($($_.Exception.Message))"
}

$want = $null
foreach ($line in Get-Content $sums) {
    if ($line -match '^([0-9a-fA-F]{64})\s+\*?(.+)$' -and $Matches[2].Trim() -eq $Asset) { $want = $Matches[1].ToLowerInvariant() }
}
# Not Get-FileHash: with pwsh 7's PSModulePath inherited, Windows PowerShell
# picks 7's Utility module, cannot load it, and the cmdlet goes missing.
$sha = [Security.Cryptography.SHA256]::Create()
$stream = [IO.File]::OpenRead($bin)
try { $got = -join ($sha.ComputeHash($stream) | ForEach-Object { $_.ToString('x2') }) } finally { $stream.Dispose(); $sha.Dispose() }
if (-not $want -or $want -ne $got) { Build-FromSource "checksum mismatch for $Asset" }

try {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
    Move-OldBinary
    Move-Item -Force $bin $Out
} catch {
    Build-FromSource "cannot install $Out ($($_.Exception.Message))"
}
Remove-Item -Recurse -Force $script:Tmp -ErrorAction SilentlyContinue
Write-Output "herdr-customize: installed the v$version release binary ($Asset)."
