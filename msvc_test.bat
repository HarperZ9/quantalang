@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" > /dev/null 2>&1
if errorlevel 1 (
    echo ERROR: vcvars64.bat failed
    exit /b 1
)
echo MSVC environment ready
cl.exe /nologo /std:c11 /c /FoNUL tests\programs\01_hello.c 2>&1
echo ---RESULT: %ERRORLEVEL%
