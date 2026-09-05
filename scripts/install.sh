#!/usr/bin/env bash
# One-line setup for Linux and macOS:
#
#   curl -fsSL https://raw.githubusercontent.com/paulo-android99/Mylib/main/scripts/install.sh | bash
#
# Downloads the prebuilt MyLib server release for this OS/architecture, makes sure FFmpeg is
# available, starts the server and prints the URL to open on this machine and on the LAN.
# Nothing is compiled locally: no Rust or Node toolchain is required.
set -euo pipefail

REPO="${MYLIB_REPO:-paulo-android99/Mylib}"
VERSION="${MYLIB_VERSION:-latest}"
AVATARS_VERSION="${MYLIB_AVATARS_VERSION:-avatars-v1}"
INSTALL_DIR="${MYLIB_INSTALL_DIR:-$HOME/mylib}"
PORT="${MYLIB_PORT:-8096}"

log() { printf '==> %s\n' "$1"; }
die() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1
}

# --- 1. Detect platform -------------------------------------------------------------------
os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
Linux) platform="linux" ;;
Darwin) platform="macos" ;;
*) die "unsupported operating system: $os (this script supports Linux and macOS; use scripts/install.ps1 on Windows)" ;;
esac

case "$arch" in
x86_64 | amd64) asset_arch="x86_64" ;;
arm64 | aarch64) asset_arch="arm64" ;;
*) die "unsupported CPU architecture: $arch" ;;
esac

if [ "$platform" = "linux" ] && [ "$asset_arch" = "arm64" ]; then
  die "no prebuilt Linux arm64 release is published yet; build from source instead (see CONTRIBUTING.md)"
fi

asset="mylib-server-${platform}-${asset_arch}.tar.gz"

# --- 2. Make sure FFmpeg/FFprobe are installed --------------------------------------------
ensure_ffmpeg() {
  if require_cmd ffmpeg && require_cmd ffprobe; then
    log "FFmpeg already installed: $(command -v ffmpeg)"
    return
  fi

  log "FFmpeg not found, installing it"
  if [ "$platform" = "macos" ]; then
    if require_cmd brew; then
      brew install ffmpeg
    else
      die "Homebrew not found. Install it from https://brew.sh, then run 'brew install ffmpeg' and re-run this script."
    fi
  else
    if require_cmd apt-get; then
      sudo apt-get update && sudo apt-get install -y ffmpeg
    elif require_cmd dnf; then
      sudo dnf install -y ffmpeg
    elif require_cmd pacman; then
      sudo pacman -Sy --noconfirm ffmpeg
    elif require_cmd zypper; then
      sudo zypper install -y ffmpeg
    else
      die "no supported package manager found (apt/dnf/pacman/zypper). Install FFmpeg manually from https://ffmpeg.org/download.html and re-run this script."
    fi
  fi

  require_cmd ffmpeg || die "FFmpeg installation did not complete; install it manually and re-run this script."
}

ensure_ffmpeg
FFMPEG_PATH="$(command -v ffmpeg)"
FFPROBE_PATH="$(command -v ffprobe)"

# --- 3. Download the release ----------------------------------------------------------------
if [ "$VERSION" = "latest" ]; then
  api_url="https://api.github.com/repos/${REPO}/releases/latest"
else
  api_url="https://api.github.com/repos/${REPO}/releases/tags/${VERSION}"
fi

log "Resolving release ${VERSION} for ${REPO}"
download_url="$(curl -fsSL "$api_url" | grep -o "\"browser_download_url\": *\"[^\"]*${asset}\"" | cut -d'"' -f4)"
[ -n "$download_url" ] || die "could not find asset ${asset} in release ${VERSION} of ${REPO}. Check https://github.com/${REPO}/releases"

mkdir -p "$INSTALL_DIR/bin" "$INSTALL_DIR/data"
tmp_archive="$(mktemp -t mylib-server.XXXXXX.tar.gz)"
trap 'rm -f "$tmp_archive"' EXIT

log "Downloading $asset"
curl -fsSL "$download_url" -o "$tmp_archive"
tar -xzf "$tmp_archive" -C "$INSTALL_DIR/bin"
chmod +x "$INSTALL_DIR/bin/mylib-server"

# --- 3b. Download the built-in avatar catalog (separate, rarely-changing release asset) ----
# Published independently of the server version via scripts/package-avatars.sh; skipped on
# reinstall/update if already present.
if [ -d "$INSTALL_DIR/data/avatars" ] && [ -n "$(ls -A "$INSTALL_DIR/data/avatars" 2>/dev/null)" ]; then
  log "Avatar catalog already present, skipping download"
else
  avatars_api_url="https://api.github.com/repos/${REPO}/releases/tags/${AVATARS_VERSION}"
  avatars_url="$(curl -fsSL "$avatars_api_url" 2>/dev/null | grep -o '"browser_download_url": *"[^"]*mylib-avatars\.tar\.gz"' | cut -d'"' -f4)"
  if [ -n "$avatars_url" ]; then
    log "Downloading avatar catalog"
    tmp_avatars="$(mktemp -t mylib-avatars.XXXXXX.tar.gz)"
    curl -fsSL "$avatars_url" -o "$tmp_avatars"
    tar -xzf "$tmp_avatars" -C "$INSTALL_DIR/data"
    rm -f "$tmp_avatars"
  else
    log "Avatar catalog release (${AVATARS_VERSION}) not found, skipping (profiles will fall back to generated avatars)"
  fi
fi

# --- 4. Start the server -------------------------------------------------------------------
log "Starting MyLib server"
export MYLIB_DATA_DIR="$INSTALL_DIR/data"
export MYLIB_HOST="0.0.0.0"
export MYLIB_PORT="$PORT"
export MYLIB_FFMPEG_PATH="$FFMPEG_PATH"
export MYLIB_FFPROBE_PATH="$FFPROBE_PATH"

nohup "$INSTALL_DIR/bin/mylib-server" >"$INSTALL_DIR/mylib.log" 2>"$INSTALL_DIR/mylib.err.log" &
server_pid=$!
echo "$server_pid" >"$INSTALL_DIR/mylib.pid"

for _ in $(seq 1 30); do
  if curl -fsS "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

if ! kill -0 "$server_pid" 2>/dev/null; then
  die "the server exited during startup; check $INSTALL_DIR/mylib.err.log"
fi

# --- 5. Report the LAN URL -------------------------------------------------------------------
lan_ip=""
if [ "$platform" = "macos" ]; then
  lan_ip="$(ipconfig getifaddr en0 2>/dev/null || ipconfig getifaddr en1 2>/dev/null || true)"
else
  lan_ip="$(hostname -I 2>/dev/null | awk '{print $1}')"
fi

echo
log "MyLib is running (pid $server_pid, logs in $INSTALL_DIR/mylib.log)"
echo "    Local:  http://localhost:${PORT}"
[ -n "$lan_ip" ] && echo "    Network: http://${lan_ip}:${PORT}"
echo
echo "Open one of the URLs above to run the first-time setup wizard."
echo "Stop the server with: kill \$(cat $INSTALL_DIR/mylib.pid)"
echo "Start it again later with: MYLIB_DATA_DIR=$INSTALL_DIR/data MYLIB_FFMPEG_PATH=$FFMPEG_PATH MYLIB_FFPROBE_PATH=$FFPROBE_PATH $INSTALL_DIR/bin/mylib-server"
