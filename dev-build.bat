@echo off
chcp 65001 >nul
rem ===== SAL 本机构建脚本 =====
rem 本机 VS18 的 vcvars64.bat 部分失效（ext 组件报错导致 VC lib 目录缺失，
rem 链接报 LNK1104 msvcrt.lib），此脚本手动补齐 LIB/INCLUDE。
rem 用法：dev-build.bat check | dev-build.bat build --release
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat"
if "%VCToolsInstallDir%"=="" set "VCToolsInstallDir=C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC\14.51.36231\"
set "LIB=%VCToolsInstallDir%lib\x64;%LIB%"
set "INCLUDE=%VCToolsInstallDir%include;%INCLUDE%"
cd /d "%~dp0src-tauri"
if "%*"=="" (cargo build) else (cargo %*)
