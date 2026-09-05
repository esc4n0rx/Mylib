# One-line setup for Windows:
#
#   irm https://raw.githubusercontent.com/esc4n0rx/Mylib/main/scripts/install.ps1 | iex
#
# Downloads the prebuilt MyLib server release for this architecture, makes sure FFmpeg is
# available, starts the server and prints the URL to open on this machine and on the LAN.
# Nothing is compiled locally: no Rust or Node toolchain is required.

param(
    [string]$Repo = "esc4n0rx/Mylib",
    [string]$Version = "latest",
    [string]$AvatarsVersion = "avatars-v1",
    [string]$InstallDir = (Join-Path $env:USERPROFILE "Mylib"),
    [int]$Port = 8096
)

$ErrorActionPreference = 'Stop'

function Write-Step($message) { Write-Host "==> $message" }

# --- 1. Detect architecture -----------------------------------------------------------------
switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { $assetArch = "x86_64" }
    "ARM64" { $assetArch = "arm64" }
    default { throw "Unsupported CPU architecture: $($env:PROCESSOR_ARCHITECTURE)" }
}
if ($assetArch -eq "arm64") {
    throw "No prebuilt Windows arm64 release is published yet; build from source instead (see CONTRIBUTING.md)."
}
$asset = "mylib-server-windows-$assetArch.zip"

# --- 2. Make sure FFmpeg/FFprobe are installed -----------------------------------------------
$ffmpeg = Get-Command ffmpeg.exe -ErrorAction SilentlyContinue
if (-not $ffmpeg) {
    Write-Step "FFmpeg not found, installing it"
    $winget = Get-Command winget.exe -ErrorAction SilentlyContinue
    if ($winget) {
        winget install --id Gyan.FFmpeg -e --source winget --accept-source-agreements --accept-package-agreements
    } else {
        $choco = Get-Command choco.exe -ErrorAction SilentlyContinue
        if ($choco) {
            choco install ffmpeg -y
        } else {
            throw "Neither winget nor Chocolatey is available. Install FFmpeg manually from https://www.gyan.dev/ffmpeg/builds/, add it to PATH, and re-run this script."
        }
    }
    # winget/choco update PATH for new shells only; refresh it for this session.
    $env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path", "User")
    $ffmpeg = Get-Command ffmpeg.exe -ErrorAction SilentlyContinue
    if (-not $ffmpeg) {
        throw "FFmpeg installation did not complete in this session. Open a new terminal and re-run this script."
    }
}
$ffmpegPath = $ffmpeg.Source
$ffprobePath = (Get-Command ffprobe.exe -ErrorAction Stop).Source
Write-Step "FFmpeg found: $ffmpegPath"

# --- 3. Download the release -----------------------------------------------------------------
if ($Version -eq "latest") {
    $apiUrl = "https://api.github.com/repos/$Repo/releases/latest"
} else {
    $apiUrl = "https://api.github.com/repos/$Repo/releases/tags/$Version"
}

Write-Step "Resolving release $Version for $Repo"
$release = Invoke-RestMethod -UseBasicParsing -Uri $apiUrl
$downloadUrl = ($release.assets | Where-Object { $_.name -eq $asset } | Select-Object -First 1).browser_download_url
if (-not $downloadUrl) {
    throw "Could not find asset $asset in release $Version of $Repo. Check https://github.com/$Repo/releases"
}

New-Item -ItemType Directory -Force -Path (Join-Path $InstallDir "bin") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $InstallDir "data") | Out-Null
$archivePath = Join-Path $env:TEMP "mylib-server.zip"

Write-Step "Downloading $asset"
Invoke-WebRequest -UseBasicParsing -Uri $downloadUrl -OutFile $archivePath
Expand-Archive -Path $archivePath -DestinationPath (Join-Path $InstallDir "bin") -Force
Remove-Item $archivePath -Force

# --- 3b. Download the built-in avatar catalog (separate, rarely-changing release asset) ------
# Published independently of the server version via scripts/package-avatars.sh; skipped on
# reinstall/update if already present.
$avatarsDir = Join-Path $InstallDir "data\avatars"
if ((Test-Path $avatarsDir) -and (Get-ChildItem $avatarsDir -ErrorAction SilentlyContinue)) {
    Write-Step "Avatar catalog already present, skipping download"
} else {
    try {
        $avatarsRelease = Invoke-RestMethod -UseBasicParsing -Uri "https://api.github.com/repos/$Repo/releases/tags/$AvatarsVersion"
        $avatarsUrl = ($avatarsRelease.assets | Where-Object { $_.name -eq "mylib-avatars.tar.gz" } | Select-Object -First 1).browser_download_url
    } catch {
        $avatarsUrl = $null
    }
    if ($avatarsUrl) {
        Write-Step "Downloading avatar catalog"
        $avatarsArchive = Join-Path $env:TEMP "mylib-avatars.tar.gz"
        Invoke-WebRequest -UseBasicParsing -Uri $avatarsUrl -OutFile $avatarsArchive
        New-Item -ItemType Directory -Force -Path (Join-Path $InstallDir "data") | Out-Null
        tar -xzf $avatarsArchive -C (Join-Path $InstallDir "data")
        Remove-Item $avatarsArchive -Force
    } else {
        Write-Step "Avatar catalog release ($AvatarsVersion) not found, skipping (profiles will fall back to generated avatars)"
    }
}

# --- 4. Start the server ----------------------------------------------------------------------
Write-Step "Starting MyLib server"
$exePath = Join-Path $InstallDir "bin\mylib-server.exe"
$env:MYLIB_DATA_DIR = Join-Path $InstallDir "data"
$env:MYLIB_HOST = "0.0.0.0"
$env:MYLIB_PORT = "$Port"
$env:MYLIB_FFMPEG_PATH = $ffmpegPath
$env:MYLIB_FFPROBE_PATH = $ffprobePath

$logOut = Join-Path $InstallDir "mylib.log"
$logErr = Join-Path $InstallDir "mylib.err.log"
$process = Start-Process -FilePath $exePath -WindowStyle Hidden -PassThru `
    -RedirectStandardOutput $logOut -RedirectStandardError $logErr
$process.Id | Out-File (Join-Path $InstallDir "mylib.pid") -Encoding ascii

$healthy = $false
for ($i = 0; $i -lt 30; $i++) {
    try {
        Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$Port/health" -TimeoutSec 2 | Out-Null
        $healthy = $true
        break
    } catch {
        Start-Sleep -Seconds 1
    }
}

if ($process.HasExited) {
    throw "The server exited during startup; check $logErr"
}
if (-not $healthy) {
    Write-Warning "Server did not answer /health yet; it may still be starting. Check $logOut"
}

# --- 5. Report the LAN URL ---------------------------------------------------------------------
$lanIp = (Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
    Where-Object { $_.IPAddress -notlike "127.*" -and $_.IPAddress -notlike "169.254.*" -and $_.PrefixOrigin -ne "WellKnown" } |
    Select-Object -First 1).IPAddress

Write-Host ""
Write-Step "MyLib is running (pid $($process.Id), logs in $logOut)"
Write-Host "    Local:   http://localhost:$Port"
if ($lanIp) { Write-Host "    Network: http://${lanIp}:$Port" }
Write-Host ""
Write-Host "Open one of the URLs above to run the first-time setup wizard."
Write-Host "Stop the server with: Stop-Process -Id $($process.Id)"
Write-Host "Start it again later by running scripts/install.ps1 again, or launching:"
Write-Host "  $exePath  (with MYLIB_DATA_DIR=$($env:MYLIB_DATA_DIR))"
