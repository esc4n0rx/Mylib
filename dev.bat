@echo off
setlocal

rem ============================================================================
rem  MyLib - inicia backend (Rust/Axum) + frontend (Vite) em janelas separadas.
rem
rem  Backend:  http://localhost:8096
rem  Frontend: http://localhost:5173  (proxy de /api e /health para o backend)
rem
rem  Chave TMDB (nao commitar chave real):
rem    1. passe como primeiro argumento:  dev.bat SUA_CHAVE_TMDB
rem    2. ou defina MYLIB_TMDB_API_KEY no ambiente antes de rodar
rem    3. ou preencha o valor padrao abaixo
rem
rem  Segredos locais (Google OAuth etc.): copie dev.local.bat.example para
rem  dev.local.bat e preencha. Esse arquivo NAO deve ser versionado.
rem ============================================================================

set "MYLIB_TMDB_API_KEY_DEFAULT=d868d61c289a4ec0691d2c299c646bd7"

set "ROOT=%~dp0"

if exist "%ROOT%dev.local.bat" call "%ROOT%dev.local.bat"

if not "%~1"=="" (
    set "MYLIB_TMDB_API_KEY=%~1"
) else if "%MYLIB_TMDB_API_KEY%"=="" (
    set "MYLIB_TMDB_API_KEY=%MYLIB_TMDB_API_KEY_DEFAULT%"
)

if "%MYLIB_TMDB_API_KEY%"=="" (
    echo [aviso] MYLIB_TMDB_API_KEY nao definida - o scan indexa arquivos sem metadados TMDB.
) else (
    echo [ok] MYLIB_TMDB_API_KEY definida.
)

if "%MYLIB_FFMPEG_PATH%"=="" set "MYLIB_FFMPEG_PATH=./tools/ffmpeg/ffmpeg.exe"
if "%MYLIB_FFPROBE_PATH%"=="" set "MYLIB_FFPROBE_PATH=./tools/ffmpeg/ffprobe.exe"

if "%MYLIB_GOOGLE_OAUTH_REDIRECT_URL%"=="" set "MYLIB_GOOGLE_OAUTH_REDIRECT_URL=http://localhost:8096/api/v1/remote-sources/google-drive/callback"

if "%MYLIB_GOOGLE_OAUTH_CLIENT_ID%"=="" (
    echo [aviso] Google Drive desativado - defina MYLIB_GOOGLE_OAUTH_CLIENT_ID/SECRET em dev.local.bat.
) else (
    echo [ok] Google Drive OAuth configurado.
)

echo FFmpeg:  %MYLIB_FFMPEG_PATH%
echo FFprobe: %MYLIB_FFPROBE_PATH%

echo Iniciando backend...
start "MyLib Backend" /d "%ROOT%." cmd /k "set MYLIB_TMDB_API_KEY=%MYLIB_TMDB_API_KEY%& set MYLIB_GOOGLE_OAUTH_CLIENT_ID=%MYLIB_GOOGLE_OAUTH_CLIENT_ID%& set MYLIB_GOOGLE_OAUTH_CLIENT_SECRET=%MYLIB_GOOGLE_OAUTH_CLIENT_SECRET%& set MYLIB_GOOGLE_OAUTH_REDIRECT_URL=%MYLIB_GOOGLE_OAUTH_REDIRECT_URL%& cargo run --bin mylib-server"

echo Iniciando frontend...
start "MyLib Frontend" /d "%ROOT%web" cmd /k "npm run dev"

echo.
echo Backend:  http://localhost:8096
echo Frontend: http://localhost:5173
endlocal
