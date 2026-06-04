#!/bin/bash
# LLM Service 测试脚本
# 用于运行 llm_service_test.rs 中的测试

# 设置 LLM 配置
# ============================================
export OPENAI_API_KEY="sk-5289550cb81f4aa7bc562bd83afb2fe3"
export LLM_MODEL="deepseek-v4-flash"
export LLM_BASE_URL="https://api.deepseek.com"

# 设置 MCP 配置（可选，留空则跳过 MCP 相关测试）
# 格式: SERVER_NAME=command:args1,args2,...
# ============================================
export MCP_SERVERS="PLAYWRIGHT=npx:@playwright/mcp@latest"

# 运行测试
# ============================================

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC_TAURI="$PROJECT_ROOT/src-tauri"

echo "========================================"
echo "LLM Service 测试"
echo "========================================"
echo ""

# 解析参数
TEST=""
IGNORED=""
REAL=""
ALL=""

while [[ $# -gt 0 ]]; do
    case $1 in
        -Test|--test)
            TEST="$2"
            shift 2
            ;;
        -Ignored|--ignored)
            IGNORED="true"
            shift
            ;;
        -Real|--real)
            REAL="true"
            shift
            ;;
        -All|--all)
            ALL="true"
            shift
            ;;
        *)
            echo "未知参数: $1"
            exit 1
            ;;
    esac
done

# 构建测试命令（使用 cmd.exe 执行 cargo）
# --release 需要放在 cargo test 之后，而不是 -- 之后
cd /d/apps/wclaw-v2/tauri-app/src-tauri
if [[ -n "$TEST" ]]; then
    TEST_CMD="cargo test --release --lib -- --ignored --nocapture $TEST"
elif [[ -n "$IGNORED" ]]; then
    TEST_CMD="cargo test --release --lib -- --ignored --nocapture"
elif [[ -n "$REAL" ]]; then
    TEST_CMD="cargo test --release --lib -- --ignored --nocapture test_intent_real_with_llm"
elif [[ -n "$ALL" ]]; then
    TEST_CMD="cargo test --release --lib -- --ignored --nocapture"
else
    TEST_CMD="cargo test --release --lib -- --nocapture llm_service_test"
fi

echo "运行命令: $TEST_CMD"
echo ""

# 使用 cmd.exe 执行，避免 Windows 路径问题
cmd.exe //c "$TEST_CMD"