#!/bin/bash
# LLM Service 简单测试脚本（不连接 MCP）

export OPENAI_API_KEY="sk-5289550cb81f4aa7bc562bd83afb2fe3"
export LLM_MODEL="deepseek-v4-flash"
export LLM_BASE_URL="https://api.deepseek.com"

cd /d/apps/wclaw-v2/tauri-app/src-tauri

echo "========================================"
echo "LLM Service 简单测试（无 MCP）"
echo "========================================"

# 运行基础 LLM 测试
cargo test --release --lib -- --ignored --nocapture test_send_message_direct