#!/usr/bin/env bash
# Packages the built-in avatar catalog (data/avatars) into a release asset.
#
# This cannot run in CI: the avatar images are not tracked in git (see .gitignore and
# AGENTS.md's "never commit data/ contents" rule) and are only ever added here, on the
# machine that already has them locally. Run it, then attach the resulting archive to a
# GitHub Release by hand:
#
#   ./scripts/package-avatars.sh
#   gh release create avatars-v1 mylib-avatars.tar.gz \
#     --title "Avatar catalog v1" --notes "Built-in profile avatar images."
#
# scripts/install.sh / scripts/install.ps1 download this asset from the release tag named
# by MYLIB_AVATARS_VERSION (default "avatars-v1") independently of the server's own version,
# so the avatar pack only needs to be re-published when the images actually change.
set -euo pipefail
cd "$(dirname "$0")/.."

src="data/avatars"
out="mylib-avatars.tar.gz"

[ -d "$src" ] || {
  echo "error: $src not found (nothing to package)" >&2
  exit 1
}

echo "==> Packaging $src -> $out"
tar -czf "$out" -C data avatars
echo "==> Done: $out ($(du -h "$out" | cut -f1))"
