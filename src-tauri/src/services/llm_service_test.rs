//! LLM Service 真实集成测试
//!
//! 用于测试意图分析和计划生成的独立测试
//! 运行真实 LLM API，需要配置环境变量
//!
//! 使用方式：
//! 1. 设置环境变量：OPENAI_API_KEY
//! 2. 运行真实测试: `cargo test --lib -- --ignored test_intent_real`

use std::sync::Arc;

use std::collections::HashMap;

use crate::provider::llm::types::{ChatMessage, ChatRequest, Role, ToolDefinition};
use crate::provider::llm::IntentAnalyzer;
use crate::provider::llm::providers::provider_trait::LlmProvider;
use crate::provider::mcp::{McpConnection, McpEventBus, TransportConfig};

// =============================================================================
// 配置加载
// =============================================================================

/// 从环境变量加载 LLM 配置
fn load_llm_config() -> (String, String, String) {
    let api_key = std::env::var("OPENAI_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .expect("请设置 OPENAI_API_KEY 或 ANTHROPIC_API_KEY 环境变量");

    let base_url = std::env::var("LLM_BASE_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

    let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());

    (base_url, api_key, model)
}

/// 从环境变量加载 MCP 配置
/// 格式: SERVER_NAME=command:args,SERVER_NAME2=command:args
/// 示例: BAIDU_BAIKE=npx.cmd:-y@modelcontextprotocol/server-baidu-baike
fn load_mcp_config() -> Vec<(String, TransportConfig)> {
    let mcp_str = std::env::var("MCP_SERVERS").unwrap_or_default();

    if mcp_str.is_empty() {
        return vec![];
    }

    mcp_str
        .split(',')
        .filter_map(|entry| {
            let parts: Vec<_> = entry.splitn(2, '=').collect();
            if parts.len() != 2 {
                return None;
            }
            let name = parts[0].trim().to_string();
            let cmd_str = parts[1].trim();

            // 解析 command:args 格式
            let cmd_parts: Vec<_> = cmd_str.splitn(2, ':').collect();
            if cmd_parts.is_empty() {
                return None;
            }

            let command = cmd_parts[0].to_string();
            let args: Vec<String> = cmd_parts
                .get(1)
                .map(|a| a.split(',').map(|s| s.to_string()).collect())
                .unwrap_or_default();

            Some((
                name,
                TransportConfig::Stdio {
                    command,
                    args,
                    env: HashMap::new(),
                },
            ))
        })
        .collect()
}

// =============================================================================
// 测试辅助函数
// =============================================================================

fn create_test_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: crate::provider::llm::types::FunctionDefinition {
                name: "mcp__baidu-baike__baike_entity_content".to_string(),
                description: Some("获取百科词条详细信息".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "百科词条义项ID"
                        }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: crate::provider::llm::types::FunctionDefinition {
                name: "mcp__baidu-baike__baike_today_in_history".to_string(),
                description: Some("获取历史上的今天发生的事".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "date": {
                            "type": "string",
                            "description": "日期，格式为MM-DD"
                        }
                    }
                }),
            },
        },
    ]
}

fn create_user_message(content: &str) -> Vec<ChatMessage> {
    vec![ChatMessage::new(Role::User, content)]
}

// =============================================================================
// 真实 LLM + MCP 集成测试（需要网络）
// =============================================================================

/// 测试：使用真实 LLM 进行意图分析
/// 需要设置环境变量：OPENAI_API_KEY
/// 运行: `cargo test --lib -- --ignored test_intent_real`
#[tokio::test]
#[ignore]
async fn test_intent_real_with_llm() {
    // 加载真实配置
    let (base_url, api_key, model) = load_llm_config();
    println!("使用模型: {} @ {}", model, base_url);

    // 创建真实 OpenAI Provider
    let provider =
        crate::provider::llm::providers::openai_compatible::OpenAiCompatible::new(
            base_url,
            api_key,
        )
        .with_model(model);

    let analyzer = IntentAnalyzer::new(Arc::new(provider));
    let messages = create_user_message("告诉我历史上的今天发生了什么？");

    // TODO(plan-module): 意图分析已不再需要工具上下文；
    // 工具选择属于 Plan / 执行阶段。analyzer.analyze 新签名已移除 available_tools 参数。
    let result = analyzer.analyze(messages).await;
    match result {
        Ok(resp) => {
            println!("意图分析结果: need_agent={}", resp.need_agent);
            println!("reasoning: {}", resp.reasoning);
            // TODO(plan-module): 当 Plan 生成模块就绪后,
            // 这里会迭代 resp.reasoning 派生出的 steps。
        }
        Err(e) => {
            panic!("意图分析失败: {:?}", e);
        }
    }
}

/// 测试：连接真实 MCP 服务器
/// 需要设置环境变量：MCP_SERVERS
/// 格式示例: MCP_SERVERS=BAIDU_BAIKE=npx.cmd:-y@modelcontextprotocol/server-baidu-baike
/// 运行: `cargo test --lib -- --ignored test_mcp_real_connection`
#[tokio::test]
#[ignore]
async fn test_mcp_real_connection() {
    let configs = load_mcp_config();

    println!("MCP 配置: {:?}", configs);
    if configs.is_empty() {
        println!("跳过 MCP 测试: 未配置 MCP_SERVERS");
        println!("设置格式: MCP_SERVERS=NAME=command:args,...");
        println!("示例: MCP_SERVERS=BAIDU_BAIKE=npx.cmd:-y@modelcontextprotocol/server-baidu-baike");
        return;
    }

    let events = Arc::new(McpEventBus::new());

    for (name, config) in configs {
        println!("连接 MCP: {} -> {:?}", name, config);
        let conn = McpConnection::new(name.clone(), config.clone(), events.clone());

        match conn.connect().await {
            Ok(status) => {
                println!("  连接成功: {:?}", status);
                match conn.list_tools().await {
                    Ok(tools) => {
                        println!("  发现 {} 个工具:", tools.len());
                        for tool in &tools {
                            println!("    - {}", tool.name);
                        }
                    }
                    Err(e) => {
                        println!("  列出工具失败: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("  连接失败: {:?}", e);
            }
        }
    }
}

/// 测试：真实 LLM + 真实 MCP 完整流程
/// 需要设置环境变量：OPENAI_API_KEY, MCP_SERVERS
/// 运行: `cargo test --release --lib -- --ignored test_full_pipeline_with_real_services`
#[tokio::test]
#[ignore]
async fn test_full_pipeline_with_real_services() {
    println!("[STEP 1] 加载配置...");
    let (base_url, api_key, model) = load_llm_config();
    let configs = load_mcp_config();
    println!("使用模型: {} @ {}", model, base_url);
    
    if configs.is_empty() {
        println!("跳过完整测试: 未配置 MCP_SERVERS");
        return;
    }

    println!("[STEP 2] 连接 MCP...");
    let events: Arc<McpEventBus> = Arc::new(McpEventBus::new());

    // 连接 MCP 服务器（只用一个）
    let mut all_tools = vec![];
    let mut conn_opt: Option<Arc<McpConnection>> = None;
    for (name, config) in &configs {
        if conn_opt.is_some() { break; }
        println!("连接 MCP: {}", name);
        // 创建 Arc 包装，确保心跳任务安全
        let conn = Arc::new(McpConnection::new_no_heartbeat(name.clone(), config.clone(), events.clone()));
        let conn_clone = Arc::clone(&conn);
        match conn_clone.connect().await {
            Ok(_) => {
                println!("  连接成功，现在列出工具...");
                match conn.list_tools().await {
                    Ok(mcp_tools) => {
                        println!("  发现 {} 个工具", mcp_tools.len());
                        for tool in mcp_tools.into_iter() {
                            let params: serde_json::Value = serde_json::json!(tool.input_schema.as_ref().clone());
                            all_tools.push(ToolDefinition::from_mcp(&tool.name, tool.description.as_deref(), params));
                        }
                        conn_opt = Some(conn);
                        break;
                    }
                    Err(e) => println!("  列出工具失败: {:?}", e),
                }
            }
            Err(e) => println!("  连接失败: {:?}", e),
        }
    }
    println!("[STEP 3] MCP 完成，{} 个工具", all_tools.len());

    if all_tools.is_empty() {
        println!("无可用工具，跳过测试");
        return;
    }

    // 创建 LLM Provider
    println!("[STEP 4] 创建 LLM Provider...");
    let provider = crate::provider::llm::providers::openai_compatible::OpenAiCompatible::new(
        base_url, api_key,
    ).with_model(model.clone());
    let analyzer = IntentAnalyzer::new(Arc::new(provider)).with_model(model);

    // 调用意图分析（不携带工具上下文；工具选择属于 Plan / 执行阶段）
    let messages = create_user_message("打开百度，搜索安仁乡，提取前3条搜索结果给我");
    let result = analyzer.analyze(messages).await;

    match result {
        Ok(resp) => {
            println!("\n=== 意图分析 ===");
            println!("need_agent: {}", resp.need_agent);
            println!("reasoning: {}", resp.reasoning);
            // TODO(plan-module): 当 Plan 生成模块就绪后,
            // 这里会打印由 resp.reasoning 派生出的 steps。
        }
        Err(e) => panic!("分析失败: {:?}", e),
    }
    
    // 清理资源
    println!("[STEP 6] 清理资源...");
    if let Some(conn) = conn_opt {
        // 使用超时断开连接，避免卡住
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), conn.disconnect()).await;
    }
    drop(events);
    println!("[STEP 6] 完成");
}

/// 测试：先连接 MCP，然后直接调用 provider.send_message
/// 运行: `cargo test --lib -- --ignored test_mcp_then_llm`
#[tokio::test]
#[ignore]
async fn test_mcp_then_llm() {
    println!("[STEP 1] 加载配置...");
    let (base_url, api_key, model) = load_llm_config();
    let configs = load_mcp_config();
    println!("使用模型: {} @ {}", model, base_url);
    
    if configs.is_empty() {
        println!("跳过: 未配置 MCP_SERVERS");
        return;
    }

    println!("[STEP 2] 连接 MCP 服务器...");
    let events: Arc<McpEventBus> = Arc::new(McpEventBus::new());
    let mut conn_arc: Option<Arc<McpConnection>> = None;

    for (name, config) in &configs {
        println!("连接 MCP: {}", name);
        let conn = Arc::new(McpConnection::new_no_heartbeat(name.clone(), config.clone(), events.clone()));
        match conn.connect().await {
            Ok(_) => {
                println!("  连接成功，现在列出工具...");
                match conn.list_tools().await {
                    Ok(mcp_tools) => {
                        println!("  发现 {} 个工具", mcp_tools.len());
                    }
                    Err(e) => {
                        println!("  列出工具失败: {:?}", e);
                    }
                }
                conn_arc = Some(conn);
                break; // 只用一个连接
            }
            Err(e) => {
                println!("  连接失败: {:?}", e);
            }
        }
    }



    println!("[STEP 4] 创建 LLM Provider...");
    let provider = crate::provider::llm::providers::openai_compatible::OpenAiCompatible::new(
        base_url,
        api_key,
    )
    .with_model(model.clone());
    println!("[STEP 4] Provider 创建成功");

    println!("[STEP 5] 调用 provider.send_message (简单请求)...");
    let req: ChatRequest = ChatRequest {
        messages: vec![ChatMessage::new(Role::User, "OK")],
        model: model,
        temperature: 0.7,
        max_tokens: Some(10),
        tools: None,
    };
    
    match provider.send_message(req).await {
        Ok(content) => {
            println!("收到响应 ({} 字符): {}", content.len(), content);
        }
        Err(e) => {
            panic!("API 调用失败: {:?}", e);
        }
    }
    
    println!("[STEP 6] 完成");
}

/// 测试：只连接 MCP，不调用 LLM
/// 运行: `cargo test --lib -- --ignored test_mcp_only`
#[tokio::test]
#[ignore]
async fn test_mcp_only() {
    println!("[STEP 1] 加载配置...");
    let configs = load_mcp_config();
    println!("MCP 配置: {:?}", configs);
    
    if configs.is_empty() {
        println!("跳过: 未配置 MCP_SERVERS");
        return;
    }

    println!("[STEP 2] 连接 MCP 服务器...");
    let events: Arc<McpEventBus> = Arc::new(McpEventBus::new());

    for (name, config) in &configs {
        println!("连接 MCP: {}", name);
        let conn = McpConnection::new_no_heartbeat(name.clone(), config.clone(), events.clone());
        match conn.connect().await {
            Ok(_) => {
                println!("  连接成功，现在列出工具...");
                match conn.list_tools().await {
                    Ok(mcp_tools) => {
                        println!("  发现 {} 个工具", mcp_tools.len());
                    }
                    Err(e) => {
                        println!("  列出工具失败: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("  连接失败: {:?}", e);
            }
        }
    }
    
    println!("[STEP 3] 清理事件总线...");
    drop(events);
    println!("[STEP 3] 完成");
}

/// 测试：IntentAnalyzer 不连接 MCP
/// 运行: `cargo test --lib -- --ignored test_analyzer_no_mcp`
#[tokio::test]
#[ignore]
async fn test_analyzer_no_mcp() {
    println!("[STEP 1] 加载配置...");
    let (base_url, api_key, model) = load_llm_config();
    println!("使用模型: {} @ {}", model, base_url);

    println!("[STEP 2] 创建 LLM Provider...");
    let provider = crate::provider::llm::providers::openai_compatible::OpenAiCompatible::new(
        base_url,
        api_key,
    )
    .with_model(model.clone());
    println!("[STEP 2] Provider 创建成功");

    println!("[STEP 3] 创建 IntentAnalyzer...");
    let analyzer = IntentAnalyzer::new(Arc::new(provider)).with_model(model);
    println!("[STEP 3] IntentAnalyzer 创建成功");

    println!("[STEP 4] 调用 analyzer.analyze...");
    let messages = create_user_message("你好，今天天气怎么样？");
    let result = analyzer.analyze(messages).await;

    match result {
        Ok(resp) => {
            println!("\n=== 意图分析 ===");
            println!("need_agent: {}", resp.need_agent);
            println!("reasoning: {}", resp.reasoning);
        }
        Err(e) => {
            panic!("失败: {:?}", e);
        }
    }
    
    println!("[STEP 5] 完成");
}

/// 测试：直接调用 send_message 验证基础 API 连接
/// 运行: `cargo test --lib -- --ignored test_send_message_direct`
#[tokio::test]
#[ignore]
async fn test_send_message_direct() {
    let (base_url, api_key, model) = load_llm_config();
    println!("使用模型: {} @ {}", model, base_url);

    let provider = crate::provider::llm::providers::openai_compatible::OpenAiCompatible::new(
        base_url,
        api_key,
    )
    .with_model(model.clone());

    let req: ChatRequest = ChatRequest {
        messages: vec![ChatMessage::new(Role::User, "你好，回复 OK")],
        model: model,
        temperature: 0.7,
        max_tokens: Some(100),
        tools: None,
    };

    println!("发送请求...");
    let result = provider.send_message(req).await;

    match result {
        Ok(content) => {
            println!("收到响应 ({} 字符): {}", content.len(), content);
            assert!(!content.is_empty(), "响应不应为空");
        }
        Err(e) => {
            panic!("API 调用失败: {:?}", e);
        }
    }
}

/// 测试：IntentAnalyzer 带 system prompt（触发崩溃的类似场景）
/// 运行: `cargo test --lib -- --ignored test_analyzer_with_system_prompt`
#[tokio::test]
#[ignore]
async fn test_analyzer_with_system_prompt() {
    let (base_url, api_key, model) = load_llm_config();
    println!("使用模型: {} @ {}", model, base_url);

    let provider = crate::provider::llm::providers::openai_compatible::OpenAiCompatible::new(
        base_url,
        api_key,
    )
    .with_model(model.clone());

    // 模拟 IntentAnalyzer 的请求格式（system + user）
    let system_prompt = "你是一个智能助手。请返回JSON格式的响应。";
    let user_message = "你好，回复 OK";

    let req: ChatRequest = ChatRequest {
        messages: vec![
            ChatMessage::new(Role::System, system_prompt),
            ChatMessage::new(Role::User, user_message),
        ],
        model: model,
        temperature: 0.7,
        max_tokens: Some(100),
        tools: None,
    };

    println!("发送请求 (2 messages: system + user)...");
    let result = provider.send_message(req).await;

    match result {
        Ok(content) => {
            println!("收到响应 ({} 字符): {}", content.len(), content);
            assert!(!content.is_empty(), "响应不应为空");
        }
        Err(e) => {
            panic!("API 调用失败: {:?}", e);
        }
    }
}

// =============================================================================
// 集成测试末尾 — IntentAnalyzer 单元测试已迁移到 `services::llm::analyze_test`，
// 那里使用 Mock LLM Provider，无需网络与真实 LLM。
// =============================================================================