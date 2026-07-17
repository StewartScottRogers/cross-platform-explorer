@echo off
setlocal

REM ============================================================
REM  herdr + Claude (native)  --  terminal+agent pair
REM  ------------------------------------------------------------
REM  Runs Claude Code INSIDE the herdr multiplexer via this repo's
REM  RunClaude.cmd launcher. Claude appears as a tracked "Claude"
REM  pane in herdr's sidebar; state comes from herdr's native
REM  Claude integration hook (installed on first run if missing).
REM
REM  Requires herdr. Model override:
REM      setx CLAUDE_MODEL "claude-sonnet-5"
REM  (default: claude-opus-4-8 -- the latest, most capable model)
REM ============================================================

if not defined CLAUDE_MODEL set "CLAUDE_MODEL=claude-opus-4-8"

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

set "HP_LABEL=Claude"
set "HP_CWD=%~dp0"
set "HP_INTEGRATION=claude"
REM  Forward the chosen model into the pane so it is honoured even if
REM  the herdr server's captured environment predates a CLAUDE_MODEL
REM  change in this shell.
set "HP_ENV=--env CLAUDE_MODEL=%CLAUDE_MODEL%"
set "HP_CMD=cmd /c "%CLAUDE_CMD%""

call "%~dp0_hrdr-launch.cmd"
endlocal
