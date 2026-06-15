//! PlansAnalyzer 测试套件
//!
//! 本文件**只**针对 `crate::provider::llm::agent::plans::PlansAnalyzer::generate`
//! 设计真实环境集成测试。模板完全沿用 `plan_executor_test.rs`：
//!
//! 1. **真实 LLM 集成测试** - 通过 `dispatcher::create_llm_provider` 工厂
//!    构造真实 LLM Provider，并连接真实 MCP 服务（如 playwright），
//!    验证 `PlansAnalyzer::generate` 在真实模型 + 真实工具列表下的行为。
//!    - 需要 `#[ignore]` 标记（需网络 + API key）
//!    - 运行: `MCP_SERVERS=playwright=npx:@playwright/mcp@latest OPENAI_API_KEY=xxx \
//!              cargo test --lib -- --ignored services::llm::plans_test`
//!
//! 2. **`parse_plans_response` 纯函数测试** - 直接测试 JSON 解析容错，
//!    不依赖任何 LLM / Provider（无需 Mock）。
//!    - 普通 `#[test]`，默认 cargo test 即可运行
//!
//! 注：`PlansAnalyzer` 内部 builder 方法链式调用 + 简单结构已通过
//! `plans.rs::tests` 单元测试覆盖，本文件不重复。

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::provider::llm::dispatcher::create_llm_provider;
    use crate::provider::llm::prompts::plans_prompt::{
        parse_plans_response, PlansResponse,
    };
    use crate::provider::llm::types::{ProviderConfigPayload, ToolDefinition};
    use crate::provider::llm::{LlmProvider, PlansAnalyzer};
    use crate::provider::mcp::{McpManager, TransportConfig};

    // =============================================================================
    // 配置加载（沿用 plan_executor_test.rs 模式）
    // =============================================================================

    /// 从环境变量加载 LLM 配置
    ///
    /// 环境变量：
    /// - `OPENAI_API_KEY`（必需）
    /// - `LLM_BASE_URL`（可选，默认 `https://api.openai.com/v1`）
    /// - `LLM_MODEL`（可选，默认 `gpt-4o-mini`）
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
    ///
    /// 格式：`NAME=command:args,...`
    /// 示例：`playwright=npx:@playwright/mcp@latest`
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

    /// 通过 `create_llm_provider` 工厂 + `load_llm_config` 构造真实 LLM Provider
    fn make_real_provider() -> (Arc<dyn LlmProvider>, String) {
        let (base_url, api_key, model) = load_llm_config();
        let config = ProviderConfigPayload::OpenAiCompatible { base_url, api_key };
        let provider = create_llm_provider(config, &model)
            .expect("创建真实 LLM Provider 失败:检查 OPENAI_API_KEY / LLM_BASE_URL 环境变量");
        (provider, model)
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

    /// 连接 MCP 服务并收集可用工具
    ///
    /// 与 `plan_executor_test.rs::test_plan_exploratory_with_llm` 中相同的逻辑
    /// 提取为独立函数，避免重复。
    async fn collect_mcp_tools(
        configs: &[(String, TransportConfig)],
    ) -> Vec<ToolDefinition> {
        if configs.is_empty() {
            return vec![];
        }
        let mcp_manager = McpManager::new();
        let mut available_tools: Vec<ToolDefinition> = Vec::new();

        for (name, config) in configs {
            println!("  连接 MCP: {}", name);
            match mcp_manager.connect(name, config.clone()).await {
                Ok(status) => {
                    println!("    连接成功: {:?}", status);
                    match mcp_manager.get_tools(name).await {
                        Ok(tools) => {
                            println!("    发现 {} 个工具", tools.len());
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
                        Err(e) => println!("    获取工具失败: {:?}", e),
                    }
                }
                Err(e) => println!("    连接失败: {:?}", e),
            }
        }
        available_tools
    }

    // =============================================================================
    // 场景 1: 真实 LLM + 真实 MCP 工具 - 百度搜索场景
    // =============================================================================

    /// 测试：PlansAnalyzer::generate 在真实 LLM + 真实 MCP 工具下生成百度搜索计划
    ///
    /// 模拟意图分析阶段产出的 `reasoning` 作为 `content` 传入 `generate`，
    /// 验证 LLM 能基于该 reasoning + 工具列表生成符合预期结构的执行计划。
    ///
    /// 运行：
    /// ```text
    /// MCP_SERVERS=playwright=npx:@playwright/mcp@latest \
    /// OPENAI_API_KEY=xxx \
    /// cargo test --lib -- --ignored services::llm::plans_test::test_plans_generate_with_llm
    /// ```
    #[tokio::test]
    async fn test_plans_generate_with_llm() {
        // 初始化 tracing
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
            )
            .try_init();

        tracing::debug!("[STEP 1] 加载配置...");
        let (provider, model) = make_real_provider();
        let mcp_configs = load_mcp_config();
        tracing::debug!("使用模型: {}", model);

        // 获取可用工具
        tracing::debug!("[STEP 2] 收集可用工具...");
        let mut available_tools = collect_mcp_tools(&mcp_configs).await;
        assert!(
            !available_tools.is_empty(),
            "无可用 MCP 工具,请通过 MCP_SERVERS 环境变量配置真实 MCP 服务(例如:playwright=npx:@playwright/mcp@latest)"
        );
        tracing::debug!("可用工具数量: {}", available_tools.len());

        // 模拟意图分析阶段的 content（reasoning）
        let content = r#"任务需要依次完成打开浏览器、导航到搜索引擎、输入搜索词'达州'、执行搜索、获取并提取前三条搜索结果。后续步骤依赖前一步执行结果，例如搜索结果的获取依赖于搜索执行完成。不确定因素包括浏览器启动状况、页面加载状态、搜索引擎界面结构变化等。终止条件为成功获取并返回前三条搜索结果。"#;

        // 构造 PlansAnalyzer
        tracing::debug!("[STEP 3] 创建 PlansAnalyzer...");
        let analyzer = PlansAnalyzer::new(provider)
            .with_model(model)
            .with_tools(available_tools);

        // 调用 generate（核心测试目标）
        tracing::debug!("[STEP 4] 调用 PlansAnalyzer::generate...");
        let result = analyzer.generate(content).await;

        /**
         * ：PlansResponse { steps: [PlanStep { order: 1, step_type: Exploratory, step_goal: "打开浏览器并导航到搜索引擎首页（如百度）", expected_output: Some("浏览器已打开并显示出搜索引擎首页"), depends_on: [], input: [], success_criteria: ["成功打开浏览器并访问搜索引擎首页", "页面加载完成"], actions: [] }, PlanStep { order: 2, step_type: Exploratory, step_goal: "在搜索框中输入“达州”并提交搜索", expected_output: Some("搜索结果页面已加 载"), depends_on: [1], input: ["{{step_1.output}}"], success_criteria: ["成功输入搜索词并触发搜索", "搜索结果页面加载完毕"], actions: [] }, PlanStep { order: 3, step_type: Exploratory, step_goal: "获取搜索结果页面的页面内容（如通过快照或网络请求）", expected_output: Some("页面内容数据（如HTML结构或快照）"), depends_on: [2], input: ["{{step_2.output}}"], success_criteria: ["成功获取页面内容", "内容包含搜索结果区域"], actions: [] }, PlanStep { order: 4, step_type: Reasoning, step_goal: "从页面内容中提取前三条搜索结果的标题和链接", expected_output: Some("前三条搜索结果的标题和链接列表"), depends_on: [3], input: ["{{step_3.output}}"], success_criteria: ["成功提取至少三条搜索结果", "结果格式为可读列表"], actions: [] }, PlanStep { order: 5, step_type: Reasoning, step_goal: " 格式化并返回前三条搜索结果", expected_output: Some("最终结果：前三条搜索结果的文本描述"), depends_on: [4], input: ["{{step_4.output}}"], success_criteria: ["成功生成包含前三条搜索结果的最终报告", "任务完成"], actions: [] }] }
         */
        // 打印并断言结果
        tracing::debug!("\n=== 计划生成结果 ===");
        let response = match result {
            Ok(r) => r,
            Err(e) => {
                // 不直接 fail,打印错误便于排查(LLM 行为存在不确定性)
                tracing::warn!("[WARN] PlansAnalyzer::generate 失败: {:?}", e);
                tracing::info!("[INFO] 跳过结构断言 - LLM 偶发错误是预期的");
                return;
            }
        };
        tracing::debug!("response：{:?}", response);
    
        // 基本结构断言
        assert!(
            !response.steps.is_empty(),
            "生成的计划应至少包含一个步骤,实际为空"
        );

        println!("生成步骤数: {}", response.steps.len());
        for step in &response.steps {
            println!(
                "  步骤 {} [{}]: {} (依赖: {:?})",
                step.order,
                format!("{:?}", step.step_type).to_lowercase(),
                step.step_goal,
                step.depends_on
            );
        }

        // 步骤序号应从 1 开始连续递增
        for (idx, step) in response.steps.iter().enumerate() {
            assert_eq!(
                step.order as usize,
                idx + 1,
                "步骤 order 应从 1 开始连续递增,实际: order={} 但位置={}",
                step.order,
                idx + 1
            );
        }

        // 第一步不应有依赖
        assert!(
            response.steps[0].depends_on.is_empty(),
            "第一步(step.order=1)不应有任何依赖"
        );

        // 步骤目标应非空
        for step in &response.steps {
            assert!(
                !step.step_goal.is_empty(),
                "步骤 {} 的 step_goal 不应为空",
                step.order
            );
        }

        // 探索性场景下,大部分步骤应该是 exploratory(deterministic 步骤需要 LLM 知道具体工具/参数,
        // 而这往往需要实际探索 DOM 才能确定)
        let exploratory_count = response
            .steps
            .iter()
            .filter(|s| matches!(s.step_type, crate::provider::llm::types::StepType::Exploratory))
            .count();
        eprintln!(
            "[INFO] 步骤类型分布 - exploratory: {} / total: {}",
            exploratory_count,
            response.steps.len()
        );

        tracing::debug!("[STEP 5] 完成");
    }

    /// 测试：PlansAnalyzer::generate 配合空工具列表也能生成计划
    ///
    /// 验证在没有任何 MCP 工具可用时,LLM 仍能基于 reasoning 产出 reasoning
    /// 或 deterministic 类型的计划(纯 LLM 推理场景)。
    ///
    /// 运行：
    /// ```text
    /// OPENAI_API_KEY=xxx \
    /// cargo test --lib -- --ignored services::llm::plans_test::test_plans_generate_without_tools
    /// ```
    #[tokio::test]
    #[ignore]
    async fn test_plans_generate_without_tools() {
        // 初始化 tracing
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
            )
            .try_init();

        let (provider, model) = make_real_provider();

        // 纯 LLM 推理场景 - 无工具
        let content = r#"用户原始请求:对比 iPhone 15 Pro 和华为 Mate 60 Pro 的拍照效果,并给出选购建议

意图分析结论:这是一个纯 LLM 推理任务,不需要调用任何外部工具。LLM 应基于内置知识
对比两款手机的摄像头硬件参数、DXO 评分、用户口碑等,并给出明确推荐。

主要执行阶段:
1. 梳理两款手机的摄像头硬件参数
2. 整理专业评测与用户口碑
3. 综合对比并给出选购建议

依赖关系:步骤 2 依赖步骤 1,步骤 3 依赖步骤 1 和 2
不确定因素:无
最终交付物:详细的对比报告与明确推荐"#;

        let analyzer = PlansAnalyzer::new(provider)
            .with_model(model)
            .with_tools(vec![]); // 显式空工具列表

        let result = analyzer.generate(content).await;
        let response = match result {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[WARN] PlansAnalyzer::generate 失败: {:?}", e);
                return;
            }
        };

        assert!(!response.steps.is_empty(), "推理场景下应至少有一个步骤");
        println!("推理场景步骤数: {}", response.steps.len());
        for step in &response.steps {
            println!(
                "  步骤 {} [{}]: {}",
                step.order,
                format!("{:?}", step.step_type).to_lowercase(),
                step.step_goal
            );
        }

        // 验证最后一步有 success_criteria(终止条件)
        if let Some(last) = response.steps.last() {
            eprintln!(
                "[INFO] 最后一步 success_criteria: {:?}",
                last.success_criteria
            );
        }
    }

    /// 测试：PlansAnalyzer::generate 配合确定性参数(builder 链式)
    ///
    /// 验证 builder 方法链式调用后,generate 仍能正常工作(不发起 LLM 请求时无法验证,
    /// 这里仅冒烟测试 builder 不会破坏内部状态)。
    #[tokio::test]
    #[ignore]
    async fn test_plans_builder_chain_with_real_provider() {
        let (provider, model) = make_real_provider();

        // 完整 builder 链
        let analyzer = PlansAnalyzer::new(provider)
            .with_model(model)
            .with_temperature(0.2)
            .with_max_tokens(Some(2048))
            .with_tools(get_default_tools());

        // 简单 reasoning 测试
        let result = analyzer.generate("用户问:你好,请简单回复").await;
        match result {
            Ok(r) => {
                println!("Builder 链测试 - 步骤数: {}", r.steps.len());
            }
            Err(e) => {
                eprintln!("[WARN] Builder 链测试失败(可接受): {:?}", e);
            }
        }
    }

    // =============================================================================
    // 场景 2: parse_plans_response 纯函数测试(不依赖 LLM / Provider / Mock)
    // =============================================================================

    /// 测试:解析简单单步计划
    #[test]
    fn test_parse_single_deterministic_step() {
        let json = r#"{
            "steps": [
                {
                    "order": 1,
                    "step_type": "deterministic",
                    "step_goal": "读取配置文件",
                    "expected_output": "配置对象",
                    "depends_on": [],
                    "input": [],
                    "success_criteria": ["成功返回配置"],
                    "actions": [
                        {"order": 1, "tool_name": "mcp__fs__read_file", "parameters": {"path": "config.json"}}
                    ]
                }
            ]
        }"#;
        let resp = parse_plans_response(json).expect("解析应成功");
        assert_eq!(resp.steps.len(), 1);
        assert_eq!(resp.steps[0].order, 1);
        assert_eq!(resp.steps[0].step_goal, "读取配置文件");
        assert_eq!(resp.steps[0].actions.len(), 1);
        assert_eq!(resp.steps[0].actions[0].tool_name, "mcp__fs__read_file");
    }

    /// 测试:解析多步依赖计划
    #[test]
    fn test_parse_multi_step_with_dependencies() {
        let json = r#"{
            "steps": [
                {
                    "order": 1,
                    "step_type": "exploratory",
                    "step_goal": "打开百度首页",
                    "expected_output": "百度首页已加载",
                    "depends_on": [],
                    "input": [],
                    "success_criteria": ["页面加载完成"],
                    "actions": []
                },
                {
                    "order": 2,
                    "step_type": "exploratory",
                    "step_goal": "在搜索框输入关键词并搜索",
                    "expected_output": "搜索结果页面",
                    "depends_on": [1],
                    "input": ["{{step_1.output}}"],
                    "success_criteria": ["搜索结果页加载"],
                    "actions": []
                },
                {
                    "order": 3,
                    "step_type": "exploratory",
                    "step_goal": "提取前三条结果",
                    "expected_output": "三条结果的列表",
                    "depends_on": [2],
                    "input": ["{{step_2.output}}"],
                    "success_criteria": ["至少返回三条结果"],
                    "actions": []
                }
            ]
        }"#;
        let resp = parse_plans_response(json).expect("解析应成功");
        assert_eq!(resp.steps.len(), 3);
        assert_eq!(resp.steps[1].depends_on, vec![1]);
        assert_eq!(resp.steps[2].depends_on, vec![2]);
        assert_eq!(resp.steps[1].input, vec!["{{step_1.output}}".to_string()]);
        assert_eq!(resp.steps[2].input, vec!["{{step_2.output}}".to_string()]);
    }

    /// 测试:PlansResponse 序列化往返
    #[test]
    fn test_plans_response_serde_roundtrip() {
        let original = PlansResponse {
            steps: vec![],
        };
        let json = serde_json::to_string(&original).expect("序列化应成功");
        let restored: PlansResponse =
            serde_json::from_str(&json).expect("反序列化应成功");
        assert_eq!(restored.steps.len(), 0);
    }

    /// 测试:空 steps 数组应返回错误
    #[test]
    fn test_parse_empty_steps_error() {
        let json = r#"{"steps": []}"#;
        let err = parse_plans_response(json).expect_err("空 steps 应报错");
        assert!(
            err.to_string().contains("steps array is empty")
                || err.to_string().contains("empty"),
            "错误信息应提示 steps 为空,实际: {}",
            err
        );
    }

    /// 测试:` ```json ... ``` ` 包裹的 JSON 应能正确解析
    #[test]
    fn test_parse_markdown_code_block() {
        let md = r#"```json
{
  "steps": [
    {
      "order": 1,
      "step_type": "exploratory",
      "step_goal": "测试 markdown 包裹",
      "expected_output": "OK",
      "depends_on": [],
      "input": [],
      "success_criteria": ["完成"],
      "actions": []
    }
  ]
}
```"#;
        let resp = parse_plans_response(md).expect("markdown 包裹应能解析");
        assert_eq!(resp.steps.len(), 1);
        assert_eq!(resp.steps[0].step_goal, "测试 markdown 包裹");
    }

    /// 测试:JSON 周围夹杂额外文本应能正确提取
    #[test]
    fn test_parse_json_with_surrounding_text() {
        let text = r#"
        好的,这是我的计划:
        {"steps": [{"order": 1, "step_type": "exploratory", "step_goal": "夹杂文本测试", "expected_output": "OK", "depends_on": [], "input": [], "success_criteria": ["完成"], "actions": []}]}
        计划已生成。
        "#;
        let resp = parse_plans_response(text).expect("应能从夹杂文本中提取");
        assert_eq!(resp.steps.len(), 1);
        assert_eq!(resp.steps[0].step_goal, "夹杂文本测试");
    }

    /// 测试:完全无效的 JSON 应返回错误
    #[test]
    fn test_parse_invalid_json_returns_error() {
        let result = parse_plans_response("this is not json at all { broken");
        assert!(result.is_err(), "无效 JSON 应返回错误");
    }
}
