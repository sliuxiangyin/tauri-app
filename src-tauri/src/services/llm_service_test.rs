//! LLM Service 真实集成测试
//!
//! 用于测试意图分析和计划生成的独立测试
//! 运行真实 LLM API，需要配置环境变量
//!
//! 使用方式：
//! 1. 设置环境变量：OPENAI_API_KEY
//! 2. 运行真实测试: `cargo test --lib -- --ignored test_intent_real`

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::Stream;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::pin::Pin;

use crate::provider::llm::error::LlmError;
use crate::provider::llm::llm_event::LlmStreamEvent;
use crate::provider::llm::providers::provider_trait::LlmStream;
use crate::provider::llm::types::{ChatMessage, ChatRequest, Role, ToolDefinition};
use crate::provider::llm::IntentAnalyzer;
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
    let tools = create_test_tools();

    let result = analyzer.analyze(messages, tools).await;
    match result {
        Ok(plan) => {
            println!("意图分析结果: need_agent={}", plan.need_agent);
            println!("reasoning: {}", plan.reasoning);
            for step in &plan.steps {
                println!("  Step {}: {} -> {}", step.order, step.tool_name, step.step_goal);
            }
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
/// 运行: `cargo test --lib -- --ignored test_full_pipeline_with_real_services`
#[tokio::test]
#[ignore]
async fn test_full_pipeline_with_real_services() {
    let (base_url, api_key, model) = load_llm_config();
    let configs = load_mcp_config();

    if configs.is_empty() {
        println!("跳过完整测试: 未配置 MCP_SERVERS");
        return;
    }

    let events: Arc<McpEventBus> = Arc::new(McpEventBus::new());

    // 1. 连接 MCP 服务器
    let mut all_tools = vec![];
    for (name, config) in &configs {
        println!("连接 MCP: {}", name);
        let conn = McpConnection::new(name.clone(), config.clone(), events.clone());
        if let Ok(_) = conn.connect().await {
            if let Ok(mcp_tools) = conn.list_tools().await {
                println!("  {} 个工具", mcp_tools.len());
                // 转换为 ToolDefinition
                for tool in mcp_tools {
                    // input_schema 是 Arc<Map>，直接克隆
                    let params: serde_json::Value = serde_json::json!(tool.input_schema.as_ref().clone());
                    
                    all_tools.push(ToolDefinition::from_mcp(
                        &tool.name,
                        tool.description.as_deref(),
                        params,
                    ));
                }
            }
        }
    }
    println!("共 {} 个工具可用", all_tools.len());

    if all_tools.is_empty() {
        println!("无可用工具，跳过测试");
        return;
    }

    // 2. 使用真实 LLM 进行意图分析
    let provider =
        crate::provider::llm::providers::openai_compatible::OpenAiCompatible::new(
            base_url,
            api_key,
        )
        .with_model(model);

    let analyzer = IntentAnalyzer::new(Arc::new(provider));

    let messages = create_user_message("打开百度，搜索安仁乡，然后给出搜索结果");
    let result = analyzer.analyze(messages, all_tools).await;

    match result {
        Ok(plan) => {
            println!("\n=== 意图计划 ===");
            println!("need_agent: {}", plan.need_agent);
            println!("reasoning: {}", plan.reasoning);
            println!("steps: {}", plan.steps.len());
            for step in &plan.steps {
                println!(
                    "  {}. [{:?}] {} - {}",
                    step.order, step.step_type, step.tool_name, step.step_goal
                );
            }
        }
        Err(e) => {
            panic!("失败: {:?}", e);
        }
    }
}

// =============================================================================
// Mock 测试（不需要网络）
// =============================================================================

/// Mock LLM Provider
struct MockLlmProvider {
    response: String,
}

impl MockLlmProvider {
    fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
        }
    }
}

#[async_trait]
impl crate::provider::llm::providers::LlmProvider for MockLlmProvider {
    async fn send_message(&self, _req: ChatRequest) -> Result<String, LlmError> {
        Ok(self.response.clone())
    }

    async fn stream_chat(
        &self,
        _req: ChatRequest,
        _abort_flag: Arc<AtomicBool>,
    ) -> Result<LlmStream, LlmError> {
        let stream = futures_util::stream::once(async move {
            Ok::<LlmStreamEvent, LlmError>(LlmStreamEvent::Done)
        });
        Ok(Box::pin(stream))
    }
}

/// 测试：简单问答（不需要工具）
#[tokio::test]
async fn test_intent_simple_question() {
    let mock_response = r#"{"need_agent":false,"reasoning":"简单问答","steps":[]}"#;
    let provider = MockLlmProvider::new(mock_response);
    let analyzer = IntentAnalyzer::new(Arc::new(provider)).with_model("gpt-4".to_string());

    let messages = create_user_message("你好，今天天气怎么样？");
    let tools = create_test_tools();

    let result = analyzer.analyze(messages, tools).await;
    assert!(result.is_ok(), "意图分析应该成功: {:?}", result.err());

    let plan = result.unwrap();
    assert!(!plan.need_agent, "简单问答不需要启用 Agent 模式");
    assert!(plan.steps.is_empty(), "简单问答不需要步骤");
}

/// 测试：需要搜索查询
#[tokio::test]
async fn test_intent_search_query() {
    let mock_response = r#"{
        "need_agent": true,
        "reasoning": "用户询问历史上的今天",
        "steps": [
            {
                "order": 1,
                "step_type": "deterministic",
                "tool_name": "mcp__baidu-baike__baike_today_in_history",
                "parameters": {"date": "05-29"},
                "step_goal": "查询5月29日的历史事件",
                "expected_output": "事件列表",
                "depends_on": null
            }
        ]
    }"#;
    let provider = MockLlmProvider::new(mock_response);
    let analyzer = IntentAnalyzer::new(Arc::new(provider)).with_model("gpt-4".to_string());

    let messages = create_user_message("历史上的今天发生了什么？");
    let tools = create_test_tools();

    let result = analyzer.analyze(messages, tools).await;
    assert!(result.is_ok());

    let plan = result.unwrap();
    assert!(plan.need_agent);
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].order, 1);
}

/// 测试：Markdown 格式响应解析
#[tokio::test]
async fn test_intent_markdown_format() {
    let mock_response = r#"```json
{"need_agent":true,"reasoning":"测试","steps":[{"order":1,"tool_name":"mcp__test__tool","parameters":{},"step_goal":"测试","depends_on":null}]}
```"#;
    let provider = MockLlmProvider::new(mock_response);
    let analyzer = IntentAnalyzer::new(Arc::new(provider)).with_model("gpt-4".to_string());

    let messages = create_user_message("测试");
    let tools = vec![];

    let result = analyzer.analyze(messages, tools).await;
    assert!(result.is_ok(), "Markdown 格式应该能正确解析");

    let plan = result.unwrap();
    assert!(plan.need_agent);
    assert_eq!(plan.steps.len(), 1);
}

/// 测试：exploratory 类型步骤（tool_name 可为空）
#[tokio::test]
async fn test_intent_exploratory_step() {
    let mock_response = r#"{
        "need_agent": true,
        "reasoning": "需要探索",
        "steps": [
            {
                "order": 1,
                "step_type": "exploratory",
                "tool_name": null,
                "parameters": {},
                "step_goal": "探索合适工具",
                "depends_on": null
            }
        ]
    }"#;
    let provider = MockLlmProvider::new(mock_response);
    let analyzer = IntentAnalyzer::new(Arc::new(provider)).with_model("gpt-4".to_string());

    let messages = create_user_message("找点有意思的");
    let tools = vec![];

    let result = analyzer.analyze(messages, tools).await;
    assert!(result.is_ok());

    let plan = result.unwrap();
    assert!(plan.steps[0]
        .step_type
        .eq(&crate::provider::llm::types::StepType::Exploratory));
    assert!(plan.steps[0].tool_name.is_empty(), "exploratory 步骤 tool_name 可为空");
}