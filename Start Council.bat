@echo off
setlocal EnableDelayedExpansion
chcp 65001 >nul
cd /d "%~dp0"
title Adversarial Council

echo ============================================================
echo   ADVERSARIAL COUNCIL  -  Mistral vs OpenRouter vs Ollama Cloud vs OpenCode Go
echo ============================================================
echo.

rem ---- 1. Find Python (install it if missing) -------------------
set "PY="
where py >nul 2>nul && set "PY=py -3"
if not defined PY ( python --version >nul 2>nul && set "PY=python" )
if not defined PY (
  echo Python is not installed yet. Installing it now - this can take a few minutes...
  echo If Windows asks for permission, click Yes.
  winget install -e --id Python.Python.3.12 --scope user --accept-package-agreements --accept-source-agreements
  echo.
  echo Done. Please CLOSE this window and double-click "Start Council" again.
  pause
  exit /b
)

rem ---- 2. First-time install of the components ------------------
if not exist ".venv\Scripts\python.exe" (
  echo First-time setup: installing components. This happens only once - about 1-2 minutes...
  %PY% -m venv .venv
  if errorlevel 1 ( echo Setup failed. Please send a screenshot of this window for help. & pause & exit /b )
  ".venv\Scripts\python.exe" -m pip install --quiet --upgrade pip
  ".venv\Scripts\python.exe" -m pip install --quiet openai python-dotenv
  if errorlevel 1 ( echo Install failed - check your internet connection and try again. & pause & exit /b )
  echo Components installed.
  echo.
)
set "VPY=.venv\Scripts\python.exe"

rem ---- 3. One-time API keys -------------------------------------
if not exist ".env" (
  echo ONE-TIME SETUP: paste your API keys.
  echo To paste: right-click inside this window, then press Enter.
  echo.
  set /p "MKEY=Mistral key: "
  set /p "ORKEY=OpenRouter key: "
  set /p "OLKEY=Ollama Cloud key: "
  set /p "OCKEY=OpenCode Go key: "
  > .env echo MISTRAL_API_KEY=!MKEY!
  >> .env echo OPENROUTER_API_KEY=!ORKEY!
  >> .env echo OLLAMA_CLOUD_API_KEY=!OLKEY!
  >> .env echo OPENCODE_API_KEY=!OCKEY!
  echo Keys saved. To change them later, open the file ".env" in this folder with Notepad.
  echo.
)

rem ---- 4. Any files dragged onto this icon become background ----
set "CTX="
for %%F in (%*) do set CTX=!CTX! --context "%%~F"

rem ---- 5. Ask the question --------------------------------------
:ask
echo.
set "Q="
set /p "Q=Type your decision question, then press Enter: "
if not defined Q goto ask
> question.txt echo(!Q!

echo.
choice /c 12 /n /m "How deep?  Press 1 = Standard (about 3-5 min)   2 = Deep (about 6-10 min): "
set "R=%errorlevel%"

echo.
"%VPY%" council.py --file question.txt --rounds %R% --open %CTX%
echo.
echo The brief has opened in your browser. All reports are kept in the "reports" folder.
pause
