//! PlanExecutor 集成测试
//!
//! 探索性步骤序列（百度搜索场景）- 使用真实 LLM + 真实 MCP 工具
//!
//! 环境变量：
//! - OPENAI_API_KEY（必需）
//! - LLM_BASE_URL（可选，默认 https://api.openai.com/v1）
//! - LLM_MODEL（可选，默认 gpt-4o-mini）
//! - MCP_SERVERS（可选，格式 NAME=command:args,...）
//!
//! 运行: `MCP_SERVERS=playwright=npx:@playwright/mcp@latest cargo test --lib -- --ignored test_plan_exploratory_with_llm`

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use crate::provider::llm::PlanExecutor;
    use crate::provider::llm::agent::plan_executor::{PlanEventCallback, PlanStreamEvent};
    use crate::provider::llm::providers::openai_compatible::OpenAiCompatible;
    use crate::provider::llm::agent::analyzer::IntentAnalyzer;
    use crate::provider::llm::types::{ChatMessage, IntentPlan, PlanStep, Role, ToolDefinition};
    use crate::provider::mcp::{McpManager, TransportConfig};
    use crate::services::llm::tool_executor::McpToolExecutor;

    // =============================================================================
    // 测试辅助 - 意图分析
    // =============================================================================

    /// 创建用户消息
    fn create_user_message(content: &str) -> Vec<ChatMessage> {
        vec![ChatMessage::new(Role::User, content)]
    }

    // =============================================================================
    // 配置加载
    // =============================================================================

    /// 从环境变量加载 LLM 配置
    fn load_llm_config() -> (String, String, String) {
        let api_key = std::env::var("OPENAI_API_KEY")
            .expect("请设置 OPENAI_API_KEY 环境变量");

        let base_url = std::env::var("LLM_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

        let model = std::env::var("LLM_MODEL")
            .unwrap_or_else(|_| "gpt-4o-mini".to_string());

        (base_url, api_key, model)
    }

    /// 从环境变量加载 MCP 配置
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
    // 测试辅助
    // =============================================================================

    /// 创建事件收集器
    fn create_event_collector() -> (Arc<std::sync::Mutex<Vec<PlanStreamEvent>>>, PlanEventCallback) {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let callback: PlanEventCallback = Arc::new(move |event| {
            events_clone.lock().unwrap().push(event);
        });
        (events, callback)
    }

    /// 获取默认工具定义（无 MCP 时使用）
    fn get_default_tools() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition::from_mcp(
                "mcp__browser__navigate",
                Some("导航到指定 URL"),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "目标 URL" }
                    },
                    "required": ["url"]
                }),
            ),
            ToolDefinition::from_mcp(
                "mcp__browser__fill",
                Some("在输入框中输入文本"),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "selector": { "type": "string", "description": "CSS 选择器" },
                        "value": { "type": "string", "description": "要输入的文本" }
                    },
                    "required": ["selector", "value"]
                }),
            ),
            ToolDefinition::from_mcp(
                "mcp__browser__click",
                Some("点击元素"),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "selector": { "type": "string", "description": "CSS 选择器" }
                    },
                    "required": ["selector"]
                }),
            ),
            ToolDefinition::from_mcp(
                "mcp__browser__extract",
                Some("提取页面内容"),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "selector": { "type": "string", "description": "CSS 选择器" }
                    },
                    "required": ["selector"]
                }),
            ),
        ]
    }

    /// 构建百度搜索场景的计划
    ///
    /// 使用 `PlanStep::exploratory` 构造器（带 builder 方法）创建探索性步骤，
    /// 避免使用已不存在的 `tool_name` / `parameters` 顶层字段（这些字段已迁移到 `SubAction`）。
    fn build_baidu_search_plan() -> IntentPlan {
        let steps = vec![
            PlanStep::exploratory(1, "打开百度首页")
                .with_expected_output("百度首页已加载"),
            PlanStep::exploratory(2, "在搜索框中输入'安仁乡'并搜索")
                .with_expected_output("搜索结果页面已加载")
                .with_dependency(1),
            PlanStep::exploratory(3, "提取前三个搜索结果的标题和链接")
                .with_expected_output("前三个搜索结果的列表")
                .with_dependency(2),
        ];
        IntentPlan::agent(steps, "百度搜索场景")
    }

    // =============================================================================
    // 真实服务集成测试 - 意图分析
    // =============================================================================

    /// 测试：使用真实 LLM + 真实 MCP 工具进行意图分析
    ///
    /// 运行: `MCP_SERVERS=playwright=npx:@playwright/mcp@latest cargo test --lib -- --ignored test_intent_analyze_with_llm`
    #[tokio::test]
    #[ignore]
    async fn test_intent_analyze_with_llm() {
        // 初始化 tracing
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
            )
            .try_init();

        tracing::debug!("[STEP 1] 加载配置1...");
        let (base_url, api_key, model) = load_llm_config();
        let configs = load_mcp_config();
        tracing::debug!("使用模型: {} @ {}", model, base_url);

        // 创建 LLM Provider
        tracing::debug!("[STEP 2] 创建 LLM Provider...");
        let llm_provider = OpenAiCompatible::new(base_url, api_key)
            .with_model(model.clone());

        // 获取可用工具
        let mcp_manager = Arc::new(McpManager::new());
        let mut available_tools: Vec<ToolDefinition> = Vec::new();

        if !configs.is_empty() {
            tracing::debug!("[STEP 3] 连接 MCP 服务...");
            for (name, config) in &configs {
                println!("  连接: {}", name);
                match mcp_manager.connect(name, config.clone()).await {
                    Ok(status) => {
                        println!("    连接成功: {:?}", status);
                        match mcp_manager.get_tools(name).await {
                            Ok(tools) => {
                                println!("    发现 {} 个工具", tools.len());
                                for tool in &tools {
                                    let tool_name = format!("mcp__{}__{}", name, tool.name);
                                    let input_schema = serde_json::Value::Object((*tool.input_schema).clone());
                                    available_tools.push(ToolDefinition::from_mcp(
                                        &tool_name,
                                        tool.description.as_deref(),
                                        input_schema,
                                    ));
                                }
                            }
                            Err(e) => println!("    获取工具失败: {:?}", e),
                        }
                    }
                    Err(e) => println!("    连接失败: {:?}", e),
                }
            }
        }

        // 无 MCP 时使用默认工具
        if available_tools.is_empty() {
            tracing::debug!("无可用 MCP 工具，使用默认工具列表");
            available_tools = get_default_tools();
        }


        // 创建 IntentAnalyzer
        println!("\n[STEP 4] 创建 IntentAnalyzer...");
        let analyzer = IntentAnalyzer::new(Arc::new(llm_provider)).with_model(model.clone());

        // 构建测试消息
        let messages = create_user_message("打开百度，输入安仁乡，获取前三条搜索结果");

        // 执行意图分析
        println!("[STEP 5] 执行意图分析...");
        // TODO(plan-module): analyzer.analyze 已移除 available_tools 参数;
        // 工具选择属于 Plan / 执行阶段。available_tools 仍可连接以便后续 Plan 模块复用。
        let result = analyzer.analyze(messages).await;

        // 打印结果
        tracing::debug!("\n=== 意图分析结果 ===");
        match result {
            Ok(resp) => {
                tracing::debug!("need_agent: {}", resp.need_agent);
                tracing::debug!("reasoning: {}", resp.reasoning);
                // TODO(plan-module): 当 Plan 生成模块就绪后,
                // 这里会迭代 resp.reasoning 派生出的 steps。
            }
            Err(e) => {
                tracing::debug!("  意图分析失败: {:?}", e);
            }
        }

        tracing::debug!("[STEP 6] 完成");
    }

    // =============================================================================
    // 真实服务集成测试 - 计划执行
    // =============================================================================

    /// 测试：探索性步骤序列（百度搜索场景）
    ///
    /// 运行: `MCP_SERVERS=playwright=npx:@playwright/mcp@latest cargo test --lib -- --ignored test_plan_exploratory_with_llm`
    #[tokio::test]
    #[ignore]
    async fn test_plan_exploratory_with_llm() {
        // 初始化 tracing
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
            )
            .try_init();

        tracing::debug!("[STEP 1] 加载配置...");
        let (base_url, api_key, model) = load_llm_config();
        let configs = load_mcp_config();
        tracing::debug!("使用模型: {} @ {}", model, base_url);

        // 创建 LLM Provider
        tracing::debug!("[STEP 2] 创建 LLM Provider...");
        let llm_provider = OpenAiCompatible::new(base_url, api_key)
            .with_model(model.clone());

        // 获取可用工具
        let mcp_manager = Arc::new(McpManager::new());
        let mut available_tools: Vec<ToolDefinition> = Vec::new();

        if !configs.is_empty() {
            tracing::debug!("[STEP 3] 连接 MCP 服务...");
            for (name, config) in &configs {
                println!("  连接: {}", name);
                match mcp_manager.connect(name, config.clone()).await {
                    Ok(status) => {
                        println!("    连接成功: {:?}", status);
                        match mcp_manager.get_tools(name).await {
                            Ok(tools) => {
                                println!("    发现 {} 个工具", tools.len());
                                for tool in &tools {
                                    let tool_name = format!("mcp__{}__{}", name, tool.name);
                                    let input_schema = serde_json::Value::Object((*tool.input_schema).clone());
                                    available_tools.push(ToolDefinition::from_mcp(
                                        &tool_name,
                                        tool.description.as_deref(),
                                        input_schema,
                                    ));
                                }
                            }
                            Err(e) => println!("    获取工具失败: {:?}", e),
                        }
                    }
                    Err(e) => println!("    连接失败: {:?}", e),
                }
            }
        }

        // 无 MCP 时使用默认工具
        if available_tools.is_empty() {
            tracing::debug!("无可用 MCP 工具，使用默认工具列表");
            available_tools = get_default_tools();
        }


        // 构建计划
        let plan = build_baidu_search_plan();
        assert_eq!(plan.steps.len(), 3);

        // 创建执行器
        println!("\n[STEP 4] 创建 PlanExecutor...");
        let (events, callback) = create_event_collector();

        let executor = PlanExecutor::new(Arc::new(McpToolExecutor::new(mcp_manager.clone())))
            .with_llm_provider(Arc::new(llm_provider))
            .with_model(model)
            .with_available_tools(available_tools)
            .with_event_callback(callback);

        let abort_flag = Arc::new(AtomicBool::new(false));

        // 执行计划
        println!("[STEP 5] 执行探索性计划...");
        let result = executor.execute_plan(plan, abort_flag).await.unwrap();

        // 打印结果
        println!("\n=== 执行结果 ===");
        println!("完成步骤: {}/{}", result.completed_steps, result.total_steps);
        println!("停止原因: {:?}", result.stop_reason);

        for step_result in &result.step_results {
            let output_preview = &step_result.output[..step_result.output.len().min(100)];
            println!(
                "  步骤 {}: {} - {}",
                step_result.order,
                if step_result.success { "成功" } else { "失败" },
                output_preview
            );
        }

        // 打印事件
        let events = events.lock().unwrap();
        println!("\n=== 事件序列 ===");
        for event in &*events {
            match event {
                PlanStreamEvent::StepStart { step, tool, goal } => {
                    println!("  StepStart: {} - tool='{}' goal='{}'", step, tool, goal);
                }
                PlanStreamEvent::StepComplete { step, success, .. } => {
                    println!("  StepComplete: {} - success={}", step, success);
                }
                PlanStreamEvent::StepError { step, error, .. } => {
                    println!("  StepError: {} - {}", step, error);
                }
                _ => {}
            }
        }

        println!("[STEP 6] 完成");
    }

    // 注: IntentAnalyzer 的单元测试已迁移到 `services::llm::analyze_test`,
    //     本文件只保留 PlanExecutor 相关的真实 LLM 集成测试。
}