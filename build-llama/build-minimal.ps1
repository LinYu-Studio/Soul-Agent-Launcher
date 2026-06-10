# build-minimal.ps1
# 极致精简版 llama.cpp 编译脚本
# CPU-only + Server + 多模态 + 流式输出

param(
    [string]$LlamaCppTag = "b9568",
    [string]$OutputDir = "$PSScriptRoot\output"
)

$ErrorActionPreference = "Stop"

# ---------- 1. 检测构建工具 ----------
Write-Host "=== 检测构建工具 ==="

# CMake
$cmake = Get-Command cmake -ErrorAction SilentlyContinue
if (-not $cmake) {
    $cmakePath = "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
    if (Test-Path $cmakePath) {
        $cmake = $cmakePath
    } else {
        throw "CMake not found. Install from https://cmake.org/download/"
    }
} else {
    $cmake = $cmake.Source
}
Write-Host "  CMake: $cmake"

# MSVC (via VS Developer Prompt)
$cl = Get-Command cl -ErrorAction SilentlyContinue
if (-not $cl) {
    # 查找 vcvars64.bat
    $vcvars = "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat"
    if (-not (Test-Path $vcvars)) {
        throw "Visual Studio 2022 not found"
    }
    Write-Host "  MSVC: $vcvars"
} else {
    Write-Host "  MSVC: $($cl.Source)"
}

# Git
$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) { throw "Git not found" }
Write-Host "  Git: $($git.Source)"

# ---------- 2. 克隆/更新 llama.cpp ----------
$srcDir = "$PSScriptRoot\llama.cpp"
if (-not (Test-Path "$srcDir\.git")) {
    Write-Host "=== 克隆 llama.cpp (tag: $LlamaCppTag) ==="
    Push-Location $PSScriptRoot
    git clone --depth 1 --branch $LlamaCppTag https://github.com/ggml-org/llama.cpp.git
    Pop-Location
} else {
    Write-Host "=== llama.cpp 已存在，跳过克隆 ==="
}

# ---------- 3. CMake 配置 ----------
$buildDir = "$PSScriptRoot\build"
if (-not (Test-Path $buildDir)) { New-Item -ItemType Directory -Path $buildDir | Out-Null }

Write-Host "=== CMake 配置（极致精简：CPU-only + Server + 多模态）==="

# 构造 CMake 命令
$cmakeArgs = @(
    "-S", $srcDir,
    "-B", $buildDir,
    "-G", "Ninja",
    "-DCMAKE_BUILD_TYPE=Release",
    "-DLLAMA_CUDA=OFF",
    "-DLLAMA_METAL=OFF",
    "-DLLAMA_VULKAN=OFF",
    "-DLLAMA_HIP=OFF",
    "-DLLAMA_SYCL=OFF",
    "-DLLAMA_CPU=ON",
    "-DLLAMA_AVX2=ON",
    "-DLLAMA_AVX=ON",
    "-DLLAMA_F16C=ON",
    "-DLLAMA_SSE3=ON",
    "-DLLAMA_SSE42=ON",
    "-DLLAMA_BUILD_SERVER=ON",
    "-DLLAMA_BUILD_TESTS=OFF",
    "-DLLAMA_BUILD_EXAMPLES=OFF",
    "-DBUILD_SHARED_LIBS=OFF",
    "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded"
)

# 用 VS 开发人员环境运行 cmake
$cmakeCmd = "`"$cmake`" $($cmakeArgs -join ' ')"

if ($vcvars) {
    $fullCmd = "`"$vcvars`" && $cmakeCmd"
    cmd /c "$fullCmd"
    if ($LASTEXITCODE -ne 0) { throw "CMake configuration failed" }
} else {
    Invoke-Expression $cmakeCmd
    if ($LASTEXITCODE -ne 0) { throw "CMake configuration failed" }
}

Write-Host "CMake configuration complete"

# ---------- 4. 编译 ----------
Write-Host "=== 编译 llama-server ==="
$buildCmd = "`"$cmake`" --build $buildDir --target llama-server --config Release -j $env:NUMBER_OF_PROCESSORS"

if ($vcvars) {
    $fullCmd = "`"$vcvars`" && $buildCmd"
    cmd /c "$fullCmd"
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
} else {
    Invoke-Expression $buildCmd
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
}

Write-Host "Build complete"

# ---------- 5. 输出 ----------
if (-not (Test-Path $OutputDir)) { New-Item -ItemType Directory -Path $OutputDir | Out-Null }

$binaryPath = "$buildDir\bin\Release\llama-server.exe"
if (-not (Test-Path $binaryPath)) {
    $binaryPath = "$buildDir\bin\llama-server.exe"
}
if (-not (Test-Path $binaryPath)) {
    throw "llama-server.exe not found after build"
}

# 复制到输出目录
Copy-Item $binaryPath "$OutputDir\llama-server.exe" -Force

# ---------- 6. UPX 压缩（可选） ----------
$upx = Get-Command upx -ErrorAction SilentlyContinue
if ($upx) {
    Write-Host "=== UPX 压缩 ==="
    & $upx --best "$OutputDir\llama-server.exe"
    Write-Host "UPX compression complete"
} else {
    Write-Host "UPX not installed, skipping compression"
    Write-Host "  Install UPX from: https://github.com/upx/upx/releases"
}

# ---------- 7. 结果统计 ----------
$fileInfo = Get-Item "$OutputDir\llama-server.exe"
Write-Host ""
Write-Host "=== 构建完成 ==="
Write-Host "  输出: $OutputDir\llama-server.exe"
Write-Host "  大小: $([math]::Round($fileInfo.Length / 1KB)) KB"
Write-Host ""
Write-Host "=== 运行时说明 ==="
Write-Host "  llama-server 已内置全部功能，但默认不启用："
Write-Host "  - 流式输出: API 传 stream: true 即可启用"
Write-Host "  - 多模态:   启动时加 --mmproj <clip-model.gguf> 加载视觉模型"
Write-Host "  - 启动示例:  llama-server -m model.gguf --host 127.0.0.1 --port 8080"
Write-Host "  - 多模态:   同上 + --mmproj mmproj-model-f16.gguf"
