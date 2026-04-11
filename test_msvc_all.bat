@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" > /dev/null 2>&1
setlocal enabledelayedexpansion
set TOTAL_ERRORS=0
set FAILED=
for %%f in (tests\programs\*.c) do (
    cl.exe /nologo /std:c11 /c /Fo:nul "%%f" > /dev/null 2> "%TEMP%\msvc_err.txt"
    if errorlevel 1 (
        set /a TOTAL_ERRORS+=1
        set FAILED=!FAILED! %%~nf
        echo FAIL: %%~nf
        type "%TEMP%\msvc_err.txt"
    )
)
echo.
echo Total failing programs: %TOTAL_ERRORS%
echo Failed: %FAILED%
