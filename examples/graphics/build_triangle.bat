@echo off
REM Build script for vulkan_triangle.c
REM Uses MSVC Build Tools + Vulkan SDK + Windows SDK

set MSVC=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207
set WINSDK=C:\Program Files (x86)\Windows Kits\10
set WINSDK_VER=10.0.26100.0
set VULKAN=C:\VulkanSDK\1.4.341.1

set CL_EXE="%MSVC%\bin\Hostx64\x64\cl.exe"
set LINK_EXE="%MSVC%\bin\Hostx64\x64\link.exe"

set INCLUDE=%VULKAN%\Include;%MSVC%\include;%WINSDK%\Include\%WINSDK_VER%\ucrt;%WINSDK%\Include\%WINSDK_VER%\um;%WINSDK%\Include\%WINSDK_VER%\shared
set LIB=%VULKAN%\Lib;%MSVC%\lib\x64;%WINSDK%\Lib\%WINSDK_VER%\ucrt\x64;%WINSDK%\Lib\%WINSDK_VER%\um\x64

echo === Building vulkan_triangle.c ===
echo.

%CL_EXE% /nologo /W4 /WX- vulkan_triangle.c /Fe:vulkan_triangle.exe /link /NOLOGO vulkan-1.lib user32.lib gdi32.lib

if %ERRORLEVEL% EQU 0 (
    echo.
    echo === BUILD SUCCEEDED ===
    echo Run: vulkan_triangle.exe
) else (
    echo.
    echo === BUILD FAILED ===
)
