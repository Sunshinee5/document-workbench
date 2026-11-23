# Document Workbench 自动验收脚本（R1 第一层：Build Gate）
# 运行方式：在项目根目录用 PowerShell 执行 .\tools\verify.ps1
# 加 -SkipTauri 可跳过 Tauri Debug 构建（本地快速自检用）
param([switch]$SkipTauri)

$ErrorActionPreference = 'Continue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$root = Split-Path -Parent $PSScriptRoot
$report = Join-Path $root 'tools\verify-report.txt'
$lines = New-Object System.Collections.Generic.List[string]
$failures = 0

function Write-Line([string]$text) {
    Write-Output $text
    $lines.Add($text) | Out-Null
}

function Invoke-Step([string]$name, [string]$command, [string]$workDir) {
    Write-Line "--- $name ---"
    Write-Line "CMD: $command"
    $started = Get-Date
    Push-Location $workDir
    try {
        $output = & cmd.exe /c $command 2>&1 | Out-String
        $code = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    $elapsed = ((Get-Date) - $started).TotalSeconds
    if ($code -eq 0) {
        Write-Line "PASS $name ($([int]$elapsed)s)"
    } else {
        Write-Line "FAIL $name (exit=$code, $([int]$elapsed)s)"
        $script:failures += 1
    }
    if ($output.Length -gt 0) {
        $tail = ($output -split "`r?`n" | Select-Object -Last 25) -join "`n"
        Write-Line $tail
    }
}

$toolchain = if ($env:DW_TOOLCHAIN) { $env:DW_TOOLCHAIN } else { '+stable-x86_64-pc-windows-gnu' }
Write-Line 'Document Workbench 验收报告'
Write-Line "时间: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
Write-Line "Rust 工具链: $toolchain（本机无 MSVC link.exe，必须带 GNU 后缀）"

Invoke-Step '格式检查' "cargo $toolchain fmt --all -- --check" $root
Invoke-Step '静态检查' "cargo $toolchain clippy --workspace --all-targets -- -D warnings" $root
Invoke-Step '全量测试' "cargo $toolchain test --workspace" $root
Invoke-Step '前端构建' 'npm run build' (Join-Path $root 'ui')

if (-not $SkipTauri) {
    Invoke-Step 'Tauri Debug 构建' "cargo $toolchain build -p dw-desktop" $root
} else {
    Write-Line 'SKIP Tauri Debug 构建（-SkipTauri）'
}

if ($failures -eq 0) {
    Write-Line 'RESULT: PASS'
} else {
    Write-Line "RESULT: FAIL ($failures 项失败)"
}

[System.IO.File]::WriteAllText($report, ($lines -join "`n") + "`n", [System.Text.Encoding]::UTF8)
exit $failures
