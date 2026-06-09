//! PlanExecutor 集成测试
//!
//! 包含 Mock 测试（plan_executor 自身逻辑验证）和真实服务集成测试。
//! 真实服务测试通过 `#[ignore]` 标记，运行前需配置环境变量：
//! - `OPENAI_API_KEY`：LLM API Key
//! - `LLM_BASE_URL`：LLM 服务地址（可选，默认 https://api.openai.com/v1）
//! - `LLM_MODEL`：模型名称（可选，默认 gpt-4o-mini）
//! - `MCP_SERVERS`：MCP 服务配置，格式 `NAME=command:args,...`
//!
//! 运行真实服务测试：
//! `MCP_SERVERS=playwright=npx:@playwright/mcp@latest cargo test --lib -- --ignored test_plan_exploratory_with_llm`

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::Value;
    use std::collections::HashMap;

    use crate::provider::llm::PlanExecutor;
use crate::provider::llm::agent::plan_executor::{
        PlanEventCallback, PlanResult, PlanStreamEvent, PlanStopReason,
    };
    use crate::provider::llm::agent::IntentAnalyzer;
    use crate::provider::llm::llm_tool_trait::{ToolExecError, ToolExecutor};
    use crate::provider::llm::providers::openai_compatible::OpenAiCompatible;
    use crate::provider::llm::types::{ChatMessage, FunctionCall, IntentPlan, PlanStep, Role};
    use crate::provider::mcp::{McpConnection, McpEventBus, McpManager, TransportConfig};
    use crate::services::llm::tool_executor::McpToolExecutor;

    // =============================================================================
    // 配置加载
    // =============================================================================

    /// 从环境变量加载 LLM 配置
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    // 测试辅助函数
    // =============================================================================

    /// Mock 工具执行器（用于纯逻辑测试）
    #[allow(dead_code)]
    struct MockToolExecutor {
        responses: HashMap<String, String>,
    }

    impl MockToolExecutor {
        fn new() -> Self {
            let mut responses = HashMap::new();
            responses.insert("mcp__test__echo".to_string(), r#""echo response""#.to_string());
            responses.insert("mcp__test__step1".to_string(), r#""step1 output""#.to_string());
            responses.insert("mcp__test__step2".to_string(), r#""step2 output""#.to_string());
            Self { responses }
        }
    }

    #[async_trait]
    impl ToolExecutor for MockToolExecutor {
        async fn execute_tool(&self, call: FunctionCall) -> Result<Value, ToolExecError> {
            if let Some(response) = self.responses.get(&call.name) {
                Ok(serde_json::json!(response))
            } else {
                Err(ToolExecError {
                    name: call.name,
                    message: "Tool not found in mock".to_string(),
                })
            }
        }
    }

    /// 创建事件收集器
    fn create_event_collector() -> (Arc<std::sync::Mutex<Vec<PlanStreamEvent>>>, PlanEventCallback) {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let callback: PlanEventCallback = Arc::new(move |event| {
            events_clone.lock().unwrap().push(event);
        });
        (events, callback)
    }

    // =============================================================================
    // Mock 测试（plan_executor 自身逻辑验证）
    // =============================================================================

    #[test]
    fn test_plan_step_builder() {
        let step = PlanStep::new(1, "mcp__server__tool", "执行某个操作")
            .with_expected_output("操作成功")
            .with_dependency(0);

        assert_eq!(step.order, 1);
        assert_eq!(step.tool_name, "mcp__server__tool");
        assert_eq!(step.step_goal, "执行某个操作");
        assert_eq!(step.expected_output, Some("操作成功".to_string()));
        assert_eq!(step.depends_on, Some(0));
    }

    #[test]
    fn test_intent_plan_simple() {
        let plan = IntentPlan::simple();
        assert!(!plan.need_agent);
        assert!(plan.steps.is_empty());
    }

    #[test]
    fn test_intent_plan_agent() {
        let steps = vec![
            PlanStep::new(1, "mcp__search__search", "搜索信息"),
            PlanStep::new(2, "mcp__browser__goto", "打开结果"),
        ];
        let plan = IntentPlan::agent(steps, "用户需要多步操作");

        assert!(plan.need_agent);
        assert_eq!(plan.steps.len(), 2);
    }

    /// 测试：简单的非 Agent 计划
    #[tokio::test]
    async fn test_plan_simple_non_agent() {
        let plan = IntentPlan::simple();
        let executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()));
        let abort_flag = Arc::new(AtomicBool::new(false));
        let result = executor.execute_plan(plan, abort_flag).await.unwrap();
        assert_eq!(result.completed_steps, 0);
        assert_eq!(result.total_steps, 0);
        assert!(result.final_reply.is_empty());
        assert_eq!(result.stop_reason, PlanStopReason::Completed);
    }

    /// 测试：空步骤列表
    #[tokio::test]
    async fn test_plan_empty_steps() {
        let plan = IntentPlan::agent(vec![], "空步骤计划");
        let executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()));
        let abort_flag = Arc::new(AtomicBool::new(false));
        let result = executor.execute_plan(plan, abort_flag).await.unwrap();
        assert_eq!(result.completed_steps, 0);
        assert_eq!(result.stop_reason, PlanStopReason::Completed);
    }

    /// 测试：使用 Mock 执行器执行简单计划
    #[tokio::test]
    async fn test_plan_with_mock_executor() {
        let plan = IntentPlan::agent(
            vec![PlanStep::new(1, "mcp__test__echo", "测试步骤")],
            "测试简单计划",
        );
        let executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()));
        let abort_flag = Arc::new(AtomicBool::new(false));
        let result = executor.execute_plan(plan, abort_flag).await.unwrap();
        assert_eq!(result.completed_steps, 1);
        assert_eq!(result.total_steps, 1);
        assert!(!result.final_reply.is_empty());
        assert_eq!(result.stop_reason, PlanStopReason::Completed);
    }

    /// 测试：带依赖的计划执行
    #[tokio::test]
    async fn test_plan_with_dependencies() {
        let plan = IntentPlan::agent(
            vec![
                PlanStep::new(1, "mcp__test__step1", "第一步"),
                PlanStep::new(2, "mcp__test__step2", "第二步").with_dependency(1),
            ],
            "测试依赖计划",
        );
        let executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()));
        let abort_flag = Arc::new(AtomicBool::new(false));
        let result = executor.execute_plan(plan, abort_flag).await.unwrap();
        assert_eq!(result.completed_steps, 2);
        assert_eq!(result.total_steps, 2);
        assert_eq!(result.stop_reason, PlanStopReason::Completed);
    }

    /// 测试：探索性步骤序列（百度搜索场景）- 使用 Mock 执行器
    /// 验证计划结构和依赖关系
    #[tokio::test]
    async fn test_plan_exploratory_steps() {
        let exploratory_steps = vec![
            PlanStep {
                order: 1,
                step_type: crate::provider::llm::types::StepType::Exploratory,
                tool_name: String::new(),
                parameters: serde_json::json!({}),
                step_goal: "打开百度首页".to_string(),
                expected_output: Some("百度首页已加载".to_string()),
                depends_on: None,
            },
            PlanStep {
                order: 2,
                step_type: crate::provider::llm::types::StepType::Exploratory,
                tool_name: String::new(),
                parameters: serde_json::json!({}),
                step_goal: "在搜索框中输入'安仁乡'并搜索".to_string(),
                expected_output: Some("搜索结果页面已加载".to_string()),
                depends_on: Some(1),
            },
            PlanStep {
                order: 3,
                step_type: crate::provider::llm::types::StepType::Exploratory,
                tool_name: String::new(),
                parameters: serde_json::json!({}),
                step_goal: "提取前三个搜索结果的标题和链接".to_string(),
                expected_output: Some("前三个搜索结果的列表".to_string()),
                depends_on: Some(2),
            },
        ];

        let plan = IntentPlan::agent(exploratory_steps, "百度搜索场景");

        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[0].depends_on, None);
        assert_eq!(plan.steps[1].depends_on, Some(1));
        assert_eq!(plan.steps[2].depends_on, Some(2));

        assert_eq!(
            plan.steps[0].step_type,
            crate::provider::llm::types::StepType::Exploratory
        );
        assert_eq!(
            plan.steps[1].step_type,
            crate::provider::llm::types::StepType::Exploratory
        );
        assert_eq!(
            plan.steps[2].step_type,
            crate::provider::llm::types::StepType::Exploratory
        );

        assert_eq!(plan.steps[0].step_goal, "打开百度首页");
        assert_eq!(plan.steps[1].step_goal, "在搜索框中输入'安仁乡'并搜索");
        assert_eq!(plan.steps[2].step_goal, "提取前三个搜索结果的标题和链接");

        // 使用 Mock 执行器（因为探索性步骤需要 LLM 决定工具）
        let executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()));
        let abort_flag = Arc::new(AtomicBool::new(false));

        // 执行计划（探索性步骤会因缺少 LLM Provider 而失败）
        let result = executor.execute_plan(plan, abort_flag).await.unwrap();

        // 验证依赖链正确（探索性步骤失败是预期行为）
        assert_eq!(result.stop_reason, PlanStopReason::PartialFailure);
    }

    /// 测试：事件回调收集
    #[tokio::test]
    async fn test_plan_event_callback() {
        let plan = IntentPlan::agent(
            vec![PlanStep::new(1, "mcp__test__echo", "测试步骤")],
            "测试事件回调",
        );
        let (events, callback) = create_event_collector();
        let executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()))
            .with_event_callback(callback);
        let abort_flag = Arc::new(AtomicBool::new(false));
        executor.execute_plan(plan, abort_flag).await.unwrap();
        let events = events.lock().unwrap();
        assert!(!events.is_empty());
    }

    /// 测试：工具不存在
    #[tokio::test]
    async fn test_plan_tool_not_found() {
        let plan = IntentPlan::agent(
            vec![PlanStep::new(1, "mcp__nonexistent__tool", "不存在的工具")],
            "测试工具不存在",
        );
        let available_tools = vec!["mcp__test__tool".to_string()];
        let executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()))
            .with_available_tools(available_tools);
        let abort_flag = Arc::new(AtomicBool::new(false));
        let result = executor.execute_plan(plan, abort_flag).await.unwrap();
        assert_eq!(result.completed_steps, 0);
        assert_eq!(result.stop_reason, PlanStopReason::ToolNotFound);
    }

    /// 测试：依赖检查失败
    #[tokio::test]
    async fn test_plan_dependency_failed() {
        let plan = IntentPlan::agent(
            vec![
                PlanStep::new(1, "mcp__test__step1", "第一步"),
                PlanStep::new(2, "mcp__test__step2", "第二步").with_dependency(99),
            ],
            "测试依赖失败",
        );
        let executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()));
        let abort_flag = Arc::new(AtomicBool::new(false));
        let result = executor.execute_plan(plan, abort_flag).await.unwrap();
        assert_eq!(result.completed_steps, 1);
        assert_eq!(result.stop_reason, PlanStopReason::DependencyFailed);
    }

    /// 测试：用户中止
    #[tokio::test]
    async fn test_plan_user_abort() {
        let steps: Vec<PlanStep> = (1..=5)
            .map(|i| PlanStep::new(i, "mcp__test__step1", format!("步骤 {}", i)))
            .collect();
        let plan = IntentPlan::agent(steps, "测试中止");
        let executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()));
        let abort_flag = Arc::new(AtomicBool::new(false));
        abort_flag.store(true, Ordering::SeqCst);
        let result = executor.execute_plan(plan, abort_flag).await.unwrap();
        assert!(result.completed_steps < 5);
        assert_eq!(result.stop_reason, PlanStopReason::UserAbort);
    }

    /// 测试：PlanExecutor Builder 链式调用
    #[test]
    fn test_plan_executor_builder() {
        let _executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()))
            .with_available_tools(vec!["tool1".to_string(), "tool2".to_string()])
            .with_max_steps(20)
            .with_max_retries(3);
        println!("PlanExecutor Builder 配置成功");
    }

    /// 测试：探索性步骤
    #[test]
    fn test_exploratory_step() {
        let step = PlanStep::exploratory(1, "探索合适工具");
        assert_eq!(step.order, 1);
        assert_eq!(
            step.step_type,
            crate::provider::llm::types::StepType::Exploratory
        );
        assert!(step.tool_name.is_empty());
        assert_eq!(step.step_goal, "探索合适工具");
    }

    // =============================================================================
    // 真实服务集成测试（需要网络和 MCP 服务）
    // =============================================================================

    /// 测试：探索性步骤序列（百度搜索场景）- 使用真实 LLM + 真实 MCP 工具
    ///
    /// 环境变量：
    /// - OPENAI_API_KEY（必需）
    /// - LLM_BASE_URL / LLM_MODEL（可选）
    /// - MCP_SERVERS（可选，格式 NAME=command:args,...）
    ///
    /// 运行: `MCP_SERVERS=playwright=npx:@playwright/mcp@latest cargo test --lib -- --ignored test_plan_exploratory_with_llm`
    #[tokio::test]
    #[ignore]
    async fn test_plan_exploratory_with_llm() {
        // 初始化 tracing，支持通过 RUST_LOG 环境变量控制日志级别
        // 例如: RUST_LOG=debug cargo test --lib -- --ignored test_plan_exploratory_with_llm
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
            )
            .with_target(true)
            .with_thread_ids(false)
            .with_file(true)
            .with_line_number(true)
            .try_init();

        tracing::debug!("[STEP 1] 加载配置...");
        let (base_url, api_key, model) = load_llm_config();
        let configs = load_mcp_config();
        tracing::debug!("使用模型: {} @ {}", model, base_url);

        // 创建 LLM Provider
        tracing::debug!("[STEP 2] 创建 LLM Provider...");
        let llm_provider = OpenAiCompatible::new(base_url, api_key)
            .with_model(model.clone());

        // 获取可用工具并建立真实 MCP 执行器
        let mcp_manager = Arc::new(McpManager::new());
        let mut available_tools = Vec::new();

        if !configs.is_empty() {
            tracing::debug!("[STEP 3] 连接 MCP 服务...");
            for (name, config) in &configs {
                println!("  连接: {}", name);
                match mcp_manager.connect(name, config.clone()).await {
                    Ok(status) => {
                        println!("    连接成功: {:?}", status);
                        // 通过 McpManager 获取工具列表
                        match mcp_manager.get_tools(name).await {
                            Ok(tools) => {
                                println!("    发现 {} 个工具", tools.len());
                                for tool in &tools {
                                    let tool_name = format!("mcp__{}__{}", name, tool.name);
                                    available_tools.push(tool_name.clone());
                                }
                            }
                            Err(e) => println!("    获取工具失败: {:?}", e),
                        }
                    }
                    Err(e) => println!("    连接失败: {:?}", e),
                }
            }
        }

        if available_tools.is_empty() {
            tracing::debug!("无可用 MCP 工具，使用模拟工具列表用于 LLM 决策");
            available_tools = vec![
                "mcp__browser__navigate".to_string(),
                "mcp__browser__fill".to_string(),
                "mcp__browser__click".to_string(),
                "mcp__browser__extract".to_string(),
            ];
        }

        tracing::debug!("\n可用工具: {:?}", available_tools);

        // 用户提供的探索性步骤数据
        let exploratory_steps = vec![
            PlanStep {
                order: 1,
                step_type: crate::provider::llm::types::StepType::Exploratory,
                tool_name: String::new(),
                parameters: serde_json::json!({}),
                step_goal: "打开百度首页".to_string(),
                expected_output: Some("百度首页已加载".to_string()),
                depends_on: None,
            },
            PlanStep {
                order: 2,
                step_type: crate::provider::llm::types::StepType::Exploratory,
                tool_name: String::new(),
                parameters: serde_json::json!({}),
                step_goal: "在搜索框中输入'安仁乡'并搜索".to_string(),
                expected_output: Some("搜索结果页面已加载".to_string()),
                depends_on: Some(1),
            },
            PlanStep {
                order: 3,
                step_type: crate::provider::llm::types::StepType::Exploratory,
                tool_name: String::new(),
                parameters: serde_json::json!({}),
                step_goal: "提取前三个搜索结果的标题和链接".to_string(),
                expected_output: Some("前三个搜索结果的列表".to_string()),
                depends_on: Some(2),
            },
        ];

        let plan = IntentPlan::agent(exploratory_steps, "百度搜索场景");

        // 验证计划结构
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[0].depends_on, None);
        assert_eq!(plan.steps[1].depends_on, Some(1));
        assert_eq!(plan.steps[2].depends_on, Some(2));

        println!("\n[STEP 4] 创建 PlanExecutor（配置 LLM Provider + McpToolExecutor）...");
        let (events, callback) = create_event_collector();

        // 使用 McpToolExecutor 替代 MockToolExecutor，接入真实的 MCP 工具
        let executor = PlanExecutor::new(Arc::new(McpToolExecutor::new(mcp_manager.clone())))
            .with_llm_provider(Arc::new(llm_provider))
            .with_available_tools(available_tools)
            .with_event_callback(callback);

        let abort_flag = Arc::new(AtomicBool::new(false));

        println!("[STEP 5] 执行探索性计划...");
        let result = executor.execute_plan(plan, abort_flag).await.unwrap();

        println!("\n=== 执行结果 ===");
        println!("完成步骤: {}/{}", result.completed_steps, result.total_steps);
        println!("停止原因: {:?}", result.stop_reason);

        // 打印步骤结果
        for step_result in &result.step_results {
            println!(
                "  步骤 {}: {} - {}",
                step_result.order,
                if step_result.success {
                    "成功"
                } else {
                    "失败"
                },
                &step_result.output[..step_result.output.len().min(100)]
            );
        }

        // 打印事件
        let events = events.lock().unwrap();
        println!("\n=== 事件序列 ===");
        for event in &*events {
            match event {
                PlanStreamEvent::StepStart {
                    step,
                    tool,
                    goal,
                } => {
                    println!("  StepStart: {} - tool='{}' goal='{}'", step, tool, goal);
                }
                PlanStreamEvent::StepComplete {
                    step,
                    success,
                    ..
                } => {
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

    /// 测试：使用真实 LLM 执行计划
    /// 运行: `cargo test --lib -- --ignored test_plan_executor_real_with_llm`
    #[tokio::test]
    #[ignore]
    async fn test_plan_executor_real_with_llm() {
        println!("[STEP 1] 加载配置...");
        let (base_url, api_key, model) = load_llm_config();
        println!("使用模型: {} @ {}", model, base_url);

        println!("[STEP 2] 创建 LLM Provider...");
        let llm_provider = OpenAiCompatible::new(base_url, api_key)
            .with_model(model.clone());

        let configs = load_mcp_config();

        if !configs.is_empty() {
            println!("[STEP 3] MCP 配置: {:?}", configs);
        }

        // 使用 Mock 执行器 + 真实 LLM Provider
        let plan = IntentPlan::agent(
            vec![PlanStep::new(1, "mcp__test__echo", "模拟步骤")],
            "模拟计划测试（真实 LLM）",
        );

        let (events, callback) = create_event_collector();
        let executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()))
            .with_llm_provider(Arc::new(llm_provider))
            .with_event_callback(callback);

        let abort_flag = Arc::new(AtomicBool::new(false));
        println!("[STEP 4] 执行计划...");
        let result = executor.execute_plan(plan, abort_flag).await.unwrap();

        println!("\n=== 执行结果 ===");
        println!(
            "完成步骤: {}/{}",
            result.completed_steps, result.total_steps
        );
        println!("停止原因: {:?}", result.stop_reason);

        let events = events.lock().unwrap();
        println!("事件数量: {}", events.len());

        println!("[STEP 5] 完成");
    }

    /// 测试：使用真实 MCP 服务（验证 MCP 连接）
    /// 运行: `MCP_SERVERS=playwright=npx:@playwright/mcp@latest cargo test --lib -- --ignored test_plan_executor_mcp_only`
    #[tokio::test]
    #[ignore]
    async fn test_plan_executor_mcp_only() {
        println!("[STEP 1] 加载 MCP 配置...");
        let configs = load_mcp_config();

        if configs.is_empty() {
            println!("跳过: 未配置 MCP_SERVERS");
            println!("设置格式: MCP_SERVERS=NAME=command:args,...");
            return;
        }

        println!("MCP 配置: {:?}", configs);

        println!("[STEP 2] 连接 MCP 服务...");
        let mcp_manager = Arc::new(McpManager::new());
        let mut available_tools = Vec::new();

        for (name, config) in &configs {
            println!("  连接: {}", name);
            match mcp_manager.connect(name, config.clone()).await {
                Ok(_) => {
                    println!("    连接成功");
                    match mcp_manager.get_tools(name).await {
                        Ok(tools) => {
                            println!("    发现 {} 个工具", tools.len());
                            for tool in &tools {
                                available_tools.push(format!("mcp__{}__{}", name, tool.name));
                            }
                        }
                        Err(e) => println!("    获取工具失败: {:?}", e),
                    }
                    break;
                }
                Err(e) => println!("    连接失败: {:?}", e),
            }
        }

        println!("\n可用工具: {:?}", available_tools);
        println!("[STEP 3] 完成");
    }

    /// 测试：意图分析 + 计划执行完整流程
    /// 运行: `cargo test --lib -- --ignored test_intent_analysis_and_execution`
    #[tokio::test]
    #[ignore]
    async fn test_intent_analysis_and_execution() {
        println!("[STEP 1] 加载配置...");
        let (base_url, api_key, model) = load_llm_config();
        println!("使用模型: {} @ {}", model, base_url);

        let llm_provider = OpenAiCompatible::new(base_url, api_key)
            .with_model(model.clone());
        let analyzer = IntentAnalyzer::new(Arc::new(llm_provider)).with_model(model);

        let tool_defs = vec![crate::provider::llm::types::ToolDefinition::from_mcp(
            "mcp__test__search",
            Some("搜索工具"),
            serde_json::json!({"type": "object", "properties": {}}),
        )];

        println!("\n=== 意图分析 ===");
        let messages = vec![ChatMessage::new(Role::User, "你好，请介绍一下自己")];
        let intent_result = analyzer.analyze(messages, tool_defs).await;

        match intent_result {
            Ok(plan) => {
                println!("意图分析结果:");
                println!("  need_agent: {}", plan.need_agent);
                println!("  reasoning: {}", plan.reasoning);
                println!("  steps: {}", plan.steps.len());

                println!("\n=== 计划执行 (Mock) ===");
                let executor = PlanExecutor::new(Arc::new(MockToolExecutor::new()))
                    .with_max_steps(10);

                let abort_flag = Arc::new(AtomicBool::new(false));
                let exec_result = executor.execute_plan(plan, abort_flag).await.unwrap();

                println!("执行结果:");
                println!(
                    "  completed: {}/{}",
                    exec_result.completed_steps, exec_result.total_steps
                );
                println!("  stop_reason: {:?}", exec_result.stop_reason);
            }
            Err(e) => println!("意图分析失败: {:?}", e),
        }

        println!("\n完成");
    }
}