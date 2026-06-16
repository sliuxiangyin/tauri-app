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
    use crate::provider::llm::agent::plan_executor::{
        PlanEventCallback, PlanResult, PlanStreamEvent,
    };
    use crate::provider::llm::providers::openai_compatible::OpenAiCompatible;
    use crate::provider::llm::types::{ChatMessage, PlanStep, Role, StepType, ToolDefinition};
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

    // =============================================================================
    // 测试用例
    // =============================================================================

    /// 构建测试用的 5 步计划：3 个 Exploratory（浏览器操作）+ 2 个 Reasoning（提取+格式化）
    fn build_test_plan() -> Vec<PlanStep> {
        vec![
            PlanStep {
                order: 1,
                step_type: StepType::Exploratory,
                step_goal: "打开浏览器并导航到搜索引擎首页（如百度）".to_string(),
                expected_output: Some("浏览器已打开并显示出搜索引擎首页".to_string()),
                depends_on: vec![],
                input: vec![],
                success_criteria: vec![
                    "成功打开浏览器并访问搜索引擎首页".to_string(),
                    "页面加载完成".to_string(),
                ],
                actions: vec![],
            },
            PlanStep {
                order: 2,
                step_type: StepType::Exploratory,
                step_goal: "在搜索框中输入「达州」并提交搜索".to_string(),
                expected_output: Some("完成搜索".to_string()),
                depends_on: vec![1],
                input: vec!["step_1.output".to_string()],
                success_criteria: vec![
                    "成功输入搜索词并触发搜索".to_string(),
                    "搜索结果页面加载完毕".to_string(),
                ],
                actions: vec![],
            },
            PlanStep {
                order: 3,
                step_type: StepType::Exploratory,
                step_goal: "获取搜索结果页面的页面内容（如通过快照或网络请求）".to_string(),
                expected_output: Some("页面内容数据（如HTML结构或快照）".to_string()),
                depends_on: vec![2],
                input: vec!["step_2.output".to_string()],
                success_criteria: vec![
                    "成功获取页面内容".to_string(),
                    "内容包含搜索结果区域".to_string(),
                ],
                actions: vec![],
            },
            PlanStep {
                order: 4,
                step_type: StepType::Reasoning,
                step_goal: "从页面内容中提取前三条搜索结果的标题和链接".to_string(),
                expected_output: Some("前三条搜索结果的标题和链接列表".to_string()),
                depends_on: vec![3],
                input: vec!["step_3.output".to_string()],
                success_criteria: vec![
                    "成功提取至少三条搜索结果".to_string(),
                    "结果格式为可读列表".to_string(),
                ],
                actions: vec![],
            },
            PlanStep {
                order: 5,
                step_type: StepType::Reasoning,
                step_goal: "格式化并返回前三条搜索结果".to_string(),
                expected_output: Some("最终结果：前三条搜索结果的文本描述".to_string()),
                depends_on: vec![4],
                input: vec!["step_4.output".to_string()],
                success_criteria: vec![
                    "成功生成包含前三条搜索结果的最终报告".to_string(),
                    "任务完成".to_string(),
                ],
                actions: vec![],
            },
        ]
    }

    /// 测试：execute_plan 执行 5 步混合计划（3 Exploratory + 2 Reasoning）
    ///
    /// 运行：
    /// ```text
    /// MCP_SERVERS=playwright=npx:@playwright/mcp@latest \
    /// OPENAI_API_KEY=xxx \
    /// cargo test --lib -- --ignored services::llm::plan_executor_test::test_execute_plan
    /// ```
    #[tokio::test]
    async fn test_execute_plan() {
        // 初始化 tracing
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
            )
            .try_init();

        tracing::info!("[STEP 1] 加载 LLM 配置...");
        let (base_url, api_key, model) = load_llm_config();
        tracing::info!("使用模型: {} @ {}", model, base_url);

        tracing::info!("[STEP 2] 创建 LLM Provider...");
        let provider = OpenAiCompatible::new(base_url, api_key).with_model(model.clone());
        let provider: Arc<dyn crate::provider::llm::providers::LlmProvider> = Arc::new(provider);
        tracing::info!("[STEP 2] Provider 创建成功");

        tracing::info!("[STEP 3] 连接 MCP 并构建工具执行器...");
        let mcp_configs = load_mcp_config();
        let mcp_manager = Arc::new(McpManager::new());
        for (name, config) in &mcp_configs {
            if let Err(e) = mcp_manager.connect(name, config.clone()).await {
                tracing::warn!("连接 MCP {} 失败: {:?}", name, e);
            }
        }
        // McpManager 实现 McpClient trait，需包装成 Arc<dyn McpClient>
        let mcp_client: Arc<dyn crate::services::traits::McpClient> = mcp_manager.clone();
        let tool_executor: Arc<dyn crate::provider::llm::llm_tool_trait::ToolExecutor> =
            Arc::new(McpToolExecutor::new(mcp_client));

        // 收集工具定义
        let mut available_tools: Vec<ToolDefinition> = Vec::new();
        for (name, _) in &mcp_configs {
            if let Ok(tools) = mcp_manager.get_tools(name).await {
                for tool in &tools {
                    let tool_name = format!("mcp__{}__{}", name, tool.name);
                    let input_schema =
                        serde_json::Value::Object((*tool.input_schema).clone());
                    available_tools.push(ToolDefinition::from_mcp(
                        &tool_name,
                        tool.description.as_deref(),
                        input_schema,
                    ));
                }
            }
        }
        if available_tools.is_empty() {
            available_tools = get_default_tools();
        }
        tracing::info!("[STEP 3] 可用工具数量: {}", available_tools.len());

        tracing::info!("[STEP 4] 创建事件收集器...");
        let (events_arc, callback) = create_event_collector();

        tracing::info!("[STEP 5] 创建 PlanExecutor...");
        let plan_executor = PlanExecutor::new(tool_executor)
            .with_llm_provider(provider)
            .with_model(model.clone())
            .with_available_tools(available_tools)
            .with_event_callback(callback)
            .with_max_retries(2)
            .with_max_exploratory_calls(5);

        tracing::info!("[STEP 6] 构建测试计划 (5 steps)...");
        let steps = build_test_plan();
        assert_eq!(steps.len(), 5, "测试计划应包含 5 个步骤");
        assert_eq!(steps[0].step_type, StepType::Exploratory);
        assert_eq!(steps[3].step_type, StepType::Reasoning);

        tracing::info!("[STEP 7] 执行计划...");
        let abort_flag = Arc::new(AtomicBool::new(false));
        let result: Result<PlanResult, _> = plan_executor
            .execute_plan(steps, abort_flag)
            .await;

        tracing::info!("[STEP 8] 验证结果...");
        match &result {
            Ok(pr) => {
                tracing::info!(
                    "执行完成: {}/{}, stop_reason={:?}, final_reply={:?}",
                    pr.completed_steps,
                    pr.total_steps,
                    pr.stop_reason,
                    pr.final_reply
                );
                for sr in &pr.step_results {
                    tracing::info!(
                        "  步骤 {}: tool={}, success={}, output_len={:?}",
                        sr.order,
                        sr.tool_name,
                        sr.success,
                        sr.output
                    );
                }
                assert_eq!(pr.total_steps, 5, "total_steps 应为 5");
                assert!(
                    pr.completed_steps >= 1,
                    "至少应完成 1 个步骤，实际: {}",
                    pr.completed_steps
                );
            }
            Err(e) => {
                tracing::error!("执行失败: {}", e);
                panic!("execute_plan 失败: {}", e);
            }
        }

        tracing::info!("[STEP 9] 验证事件流...");
        let events = events_arc.lock().unwrap();
        tracing::info!("事件总数: {}", events.len());
        assert!(!events.is_empty(), "应至少发出 PlanStart 事件");

        let mut has_plan_start = false;
        let mut step_start_count = 0;
        let mut step_complete_count = 0;
        for ev in events.iter() {
            match ev {
                PlanStreamEvent::PlanStart { .. } => has_plan_start = true,
                PlanStreamEvent::StepStart { .. } => step_start_count += 1,
                PlanStreamEvent::StepComplete { .. } => step_complete_count += 1,
                _ => {}
            }
        }
        assert!(has_plan_start, "应包含 PlanStart 事件");
        assert!(
            step_start_count >= 1,
            "应至少有 1 个 StepStart 事件，实际: {}",
            step_start_count
        );
        tracing::info!(
            "事件统计: PlanStart={}, StepStart={}, StepComplete={}",
            has_plan_start,
            step_start_count,
            step_complete_count
        );
    }
}