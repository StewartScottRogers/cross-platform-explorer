@echo off
setlocal

REM ============================================================
REM  herdr + Claude (native)  --  terminal+agent pair
REM  ------------------------------------------------------------
REM  Launches Claude Code as a tracked "Claude" agent pane inside
REM  the herdr multiplexer, via this repo's RunClaude.cmd launcher.
REM  Claude appears as a tracked "Claude" pane in herdr's sidebar;
REM  state comes from herdr's native Claude integration hook, which
REM  is installed here on first run if it is missing.
REM
REM  Requires herdr on PATH (and a running herdr session). Model
REM  override:
REM      setx CLAUDE_MODEL "claude-sonnet-5"
REM  (default: claude-opus-4-8 -- the latest, most capable model)
REM ============================================================

if not defined CLAUDE_MODEL set "CLAUDE_MODEL=claude-opus-4-8"

REM  Require herdr on PATH.
where herdr >nul 2>nul
if errorlevel 1 (
    echo ERROR: herdr was not found on PATH.
    echo     Install herdr, or add its bin directory to PATH, then retry.
    echo.
    pause
    endlocal
    exit /b 1
)

REM  Use this repo's RunClaude.cmd as the in-pane command so the
REM  Claude Code launch (flags, model, cwd) stays in one place.
set "CLAUDE_CMD=%~dp0RunClaude.cmd"
for %%I in ("%CLAUDE_CMD%") do set "CLAUDE_CMD=%%~fI"
if not exist "%CLAUDE_CMD%" (
    echo ERROR: could not find RunClaude.cmd at:
    echo     %CLAUDE_CMD%
    echo.
    pause
    endlocal
    exit /b 1
)

REM  Repo dir, without the trailing backslash so it survives argument
REM  quoting when passed to herdr.
set "REPO_DIR=%~dp0"
if "%REPO_DIR:~-1%"=="\" set "REPO_DIR=%REPO_DIR:~0,-1%"

REM  Ensure herdr's native Claude integration hook is installed so the
REM  pane reports Claude's state (idle/working/blocked) in the sidebar.
herdr integration status 2>nul | findstr /b /c:"claude: current" >nul
if errorlevel 1 herdr integration install claude

REM  Launch Claude as a tracked "Claude" agent pane. --cwd anchors it to
REM  this repo; --env forwards the chosen model so it is honoured even if
REM  the herdr server's captured environment predates a CLAUDE_MODEL
REM  change in this shell.
herdr agent start Claude --cwd "%REPO_DIR%" --env CLAUDE_MODEL=%CLAUDE_MODEL% --focus -- cmd /c "%CLAUDE_CMD%"
if errorlevel 1 (
    echo.
    echo ERROR: herdr could not start the Claude pane. Is a herdr session running?
    echo     Start herdr first ^(run: herdr^), then retry this script.
    echo.
    pause
    endlocal
    exit /b 1
)

endlocal
