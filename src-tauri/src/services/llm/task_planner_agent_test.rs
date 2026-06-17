//! TaskPlannerAgent 非流式测试
//!
//! 环境变量：
//! - OPENAI_API_KEY（必需）
//! - LLM_BASE_URL（可选，默认 https://api.openai.com/v1）
//! - LLM_MODEL（可选，默认 gpt-4o-mini）
//!
//! 运行：
//! ```text
//! OPENAI_API_KEY=xxx cargo test --lib -- --ignored test_task_planner_run
//! ```

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::provider::llm::planner::task_planner_agent::agent::TaskPlannerAgent;
    use crate::provider::llm::providers::openai_compatible::OpenAiCompatible;
    use crate::provider::llm::providers::provider_trait::LlmProvider;

    /// 从环境变量加载 LLM 配置
    fn load_llm_config() -> (String, String, String) {
        let api_key =
            std::env::var("OPENAI_API_KEY").expect("请设置 OPENAI_API_KEY 环境变量");

        let base_url = std::env::var("LLM_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

        let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());

        (base_url, api_key, model)
    }

    /// 测试 TaskPlannerAgent.run()
    ///
    /// 验证：
    /// 1. run() 直接返回完整结构化 TaskPlan
    /// 2. TaskPlan 包含至少 1 个 Stage
    /// 3. 每个 Stage 的 id / domain / goal 非空
    #[tokio::test]
    async fn test_task_planner_run() {
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
        let provider: Arc<dyn LlmProvider> = Arc::new(provider);
        tracing::info!("[STEP 3] 创建 TaskPlannerAgent...");
        let user_request: &str = "在百度搜索 AI 新闻并提取前三条结果摘要";
        let agent = TaskPlannerAgent::new(provider)
            .with_model(model)
            .with_temperature(0.1)
            .with_available_domains(vec![
                "browser".into(),
                "file".into(),
                "http".into(),
                "adb".into(),
            ])
            .with_conversation_context(String::new())
            .with_user_request(user_request.into());

        tracing::info!("[STEP 4] 调用 run()...");
        let plan = agent
            .run()
            .await
            .expect("run() 应成功返回 TaskPlan");

        tracing::info!(
            "[STEP 5] 验证 TaskPlan: {:?}",
            plan.stages
        );
        assert!(
            !plan.stages.is_empty(),
            "TaskPlan 应至少包含 1 个 Stage"
        );

        for (i, stage) in plan.stages.iter().enumerate() {
            tracing::info!(
                "  Stage {}: id={}, domain={}, goal={}",
                i,
                stage.id,
                stage.domain,
                stage.goal
            );
            assert!(!stage.id.is_empty(), "Stage id 不应为空");
            assert!(!stage.domain.is_empty(), "Stage domain 不应为空");
            assert!(!stage.goal.is_empty(), "Stage goal 不应为空");
        }

        tracing::info!("[TEST PASSED] TaskPlannerAgent.run() 测试通过");
    }
}
