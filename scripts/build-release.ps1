# Produces a single release artifact: web bundle embedded into the server binary.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

Write-Host '==> Building web frontend'
npm --prefix web ci
npm --prefix web run build

Write-Host '==> Building server (embeds web/dist)'
cargo build --release

Write-Host '==> Done: target/release/mylib-server'
