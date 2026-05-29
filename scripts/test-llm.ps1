# LLM Service 测试脚本
# 用于运行 llm_service_test.rs 中的测试

param(
    [string]$Test = "",
    [switch]$Ignored,
    [switch]$Real,
    [switch]$All
)

# 设置 LLM 配置
# ============================================
$env:OPENAI_API_KEY = "sk-5289550cb81f4aa7bc562bd83afb2fe3"
$env:LLM_MODEL = "deepseek-v4-flash"
$env:LLM_BASE_URL = "https://api.deepseek.com"

# 设置 MCP 配置（可选，留空则跳过 MCP 相关测试）
# 格式: SERVER_NAME=command:args,SERVER_NAME2=command:args
# ============================================
$env:MCP_SERVERS = "PLAYWRIGHT=npx.cmd:-y@playwright/mcp@latest"

# 运行测试
# ============================================

$projectRoot = Split-Path -Parent $PSScriptRoot
$srcTauri = Join-Path $projectRoot "src-tauri"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "LLM Service 测试" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# 构建测试命令
$testCmd = "cargo test --lib --manifest-path `"$srcTauri\Cargo.toml`""

if ($Test -ne "") {
    $testCmd += " $Test"
} elseif ($Ignored) {
    $testCmd += " -- --ignored"
} elseif ($Real) {
    $testCmd += " -- --ignored test_intent_real_with_llm"
} elseif ($All) {
    $testCmd += " -- --ignored"
} else {
    $testCmd += " llm_service_test"
}

Write-Host "运行命令: $testCmd" -ForegroundColor Gray
Write-Host ""

# 执行测试
Set-Location $srcTauri
Invoke-Expression $testCmd