@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" > /dev/null 2>&1
cd C:\Users\Zain\QUANTA-UNIVERSE\quantalang
cl.exe /nologo /std:c11 /Fe:tests\programs\47_uniform_buffers.exe tests\programs\47_uniform_buffers.c
