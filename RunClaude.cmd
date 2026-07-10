@echo off
setlocal
pushd "%~dp0"
call claude --dangerously-skip-permissions --verbose
popd
endlocal
