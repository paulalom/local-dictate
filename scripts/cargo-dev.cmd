@echo off
setlocal

set "VSDEVCMD=%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"
set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"

if not exist "%CARGO%" (
    echo cargo.exe was not found at %CARGO% 1>&2
    exit /b 1
)

if exist "%VSDEVCMD%" (
    call "%VSDEVCMD%" -arch=x64 >nul
)

"%CARGO%" %*
