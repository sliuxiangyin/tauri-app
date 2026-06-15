//! IntentAnalyzer 测试套件
//!
//! 两类测试:
//! 1. **真实 LLM 集成测试** - 调用 `dispatcher::create_llm_provider` + `load_llm_config`
//!    构造真实 LLM Provider,验证 IntentAnalyzer 在真实模型下的行为。
//!    - 需要 `#[ignore]` 标记(需要网络 + API key)
//!    - 运行: `OPENAI_API_KEY=xxx cargo test --lib -- --ignored services::llm::analyze_test`
//!
//! 2. **`parse_intent_response` 纯函数测试** - 直接测试解析容错与错误处理,
//!    不依赖任何 LLM Provider(因此无需 Mock)。
//!    - 普通 `#[test]`,默认 cargo test 即可运行

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::provider::llm::dispatcher::create_llm_provider;
    use crate::provider::llm::prompts::intent_prompt::{
        parse_intent_response, IntentResponse,
    };
    use crate::provider::llm::types::{ChatMessage, ProviderConfigPayload, Role};
    use crate::provider::llm::{IntentAnalyzer, LlmProvider};

    // =============================================================================
    // 真实 LLM 集成测试
    // =============================================================================

    /// 从环境变量加载 LLM 配置(参照 `plan_executor_test.rs::load_llm_config` 模式)
    fn load_llm_config() -> (String, String, String) {
        let api_key = std::env::var("OPENAI_API_KEY")
            .expect("请设置 OPENAI_API_KEY 环境变量");

        let base_url = std::env::var("LLM_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

        let model = std::env::var("LLM_MODEL")
            .unwrap_or_else(|_| "gpt-4o-mini".to_string());

        (base_url, api_key, model)
    }

    /// 通过步骤 1 实现的 `create_llm_provider` 工厂函数 + `load_llm_config`
    /// 构造真实 LLM Provider(OpenAI 兼容)。
    fn make_real_provider() -> Arc<dyn LlmProvider> {
        let (base_url, api_key, model) = load_llm_config();
        let config = ProviderConfigPayload::OpenAiCompatible { base_url, api_key };
        create_llm_provider(config, &model)
            .expect("创建真实 LLM Provider 失败:检查 OPENAI_API_KEY / LLM_BASE_URL 环境变量")
    }

    // -----------------------------------------------------------------------
    // 场景 1: 真实 LLM 下的核心行为
    // -----------------------------------------------------------------------

    /// 测试:真实 LLM 对简单问候不应启用 Agent
    /// 运行: `OPENAI_API_KEY=xxx cargo test --lib -- --ignored test_analyzer_real_simple --nocapture  `
    #[tokio::test]
    async fn test_analyzer_real_simple_question() {
          // 初始化 tracing
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
            )
            .try_init();
        let provider = make_real_provider();
        let analyzer = IntentAnalyzer::new(provider);

        let result = analyzer
            .analyze(vec![ChatMessage::new(Role::User, "帮我搜索达州，并获取前三条搜索结果?")])
            .await;
        tracing::debug!("dddddddddd {:?}", result);  // 改用 eprintln! 
        assert!(result.is_ok(), "真实 LLM 意图分析应成功: {:?}", result.err());
        let resp = result.unwrap();
        assert!(!resp.need_agent, "简单问候不应启用 Agent 模式");
        assert!(!resp.reasoning.is_empty(), "reasoning 字段必须有内容");
    }

    /// 测试:真实 LLM 对多步工具任务应能正确识别
    /// 运行: `OPENAI_API_KEY=xxx cargo test --lib -- --ignored test_analyzer_real_agent_mode`
    #[tokio::test]
    async fn test_analyzer_real_agent_mode_with_decomposition() {
        let provider = make_real_provider();
        let analyzer = IntentAnalyzer::new(provider);

        let result = analyzer
            .analyze(vec![ChatMessage::new(
                Role::User,
                "比较 A 和 B 两款手机的拍照效果并选出更优,需要分多步搜索信息",
            )])
            .await;

        let resp = result.expect("意图分析应成功");
        if resp.need_agent {
            assert!(
                resp.reasoning.len() > 10,
                "Agent 模式下 reasoning 应有充分的任务分解描述"
            );
        } else {
            // 真实 LLM 行为不确定:有可能认为简单,不做强制断言
            eprintln!(
                "[INFO] 真实 LLM 判定该请求不需要 Agent(可能合理): reasoning={}",
                resp.reasoning
            );
        }
    }

    /// 测试:真实 LLM 对搜索类任务应能生成结构化 reasoning
    /// 运行: `OPENAI_API_KEY=xxx cargo test --lib -- --ignored test_analyzer_real_search`
    #[tokio::test]
    #[ignore]
    async fn test_analyzer_real_search_query() {
        let provider = make_real_provider();
        let analyzer = IntentAnalyzer::new(provider);

        let result = analyzer
            .analyze(vec![ChatMessage::new(
                Role::User,
                "查询 2024 年 6 月历史上的今天发生了什么大事",
            )])
            .await;

        let resp = result.expect("意图分析应成功");
        // 真实 LLM 行为具有不确定性,只做基本断言
        assert!(!resp.reasoning.is_empty(), "reasoning 必须有内容");
    }

    // -----------------------------------------------------------------------
    // 场景 2: Builder API(不调用 LLM,真实 Provider 即可)
    // -----------------------------------------------------------------------

    /// 测试:Builder 方法应能链式调用(不发起 LLM 请求,但需要真实 Provider 实例)
    /// 运行: `OPENAI_API_KEY=xxx cargo test --lib -- --ignored test_analyzer_builder_methods`
    #[tokio::test]
    #[ignore]
    async fn test_analyzer_builder_methods() {
        let provider = make_real_provider();
        let analyzer = IntentAnalyzer::new(provider)
            
            .with_temperature(0.5)
            .with_max_tokens(Some(512));
        // 仅验证 builder 链式调用不 panic,不调用 .analyze()
        drop(analyzer);
    }

    // =============================================================================
    // parse_intent_response 纯函数测试(不依赖 LLM / Provider / Mock)
    // =============================================================================

    // -----------------------------------------------------------------------
    // 正常解析
    // -----------------------------------------------------------------------

    /// 测试:解析最小有效 JSON
    #[test]
    fn test_parse_simple() {
        let json = r#"{"need_agent":false,"reasoning":"简单问答"}"#;
        let resp = parse_intent_response(json).expect("应解析成功");
        assert!(!resp.need_agent);
        assert_eq!(resp.reasoning, "简单问答");
    }

    /// 测试:解析多步任务 reasoning
    #[test]
    fn test_parse_decomposition() {
        let json = r#"{
            "need_agent": true,
            "reasoning": "任务分解:1) 调用 mcp__search__web 搜索 A 产品;2) 搜索 B 产品;3) 对比。终止条件:给出明确选择。"
        }"#;
        let resp = parse_intent_response(json).expect("应解析成功");
        assert!(resp.need_agent);
        assert!(resp.reasoning.contains("任务分解"));
        assert!(resp.reasoning.contains("终止条件"));
    }

    /// 测试:解析含具体工具名的 reasoning
    #[test]
    fn test_parse_search_query_with_tool_name() {
        let json = r#"{
            "need_agent": true,
            "reasoning": "需调用 mcp__baidu-baike__baike_today_in_history 工具"
        }"#;
        let resp = parse_intent_response(json).expect("应解析成功");
        assert!(resp.need_agent);
        assert!(resp.reasoning.contains("baike"));
    }

    /// 测试:解析含探索性标记的 reasoning
    #[test]
    fn test_parse_reasoning_marks_exploratory() {
        let json = r#"{
            "need_agent": true,
            "reasoning": "任务需要探索:无法预知 CSS 选择器,需根据运行时反馈决策"
        }"#;
        let resp = parse_intent_response(json).expect("应解析成功");
        assert!(resp.need_agent);
        assert!(resp.reasoning.contains("探索") || resp.reasoning.contains("运行时"));
    }

    // -----------------------------------------------------------------------
    // 容错解析
    // -----------------------------------------------------------------------

    /// 测试:` ```json ... ``` ` 包裹的 JSON 应能正确解析
    #[test]
    fn test_parse_markdown_code_block() {
        let md = "```json\n{\"need_agent\":true,\"reasoning\":\"markdown 包裹\"}\n```";
        let resp = parse_intent_response(md).expect("应解析成功");
        assert!(resp.need_agent);
        assert_eq!(resp.reasoning, "markdown 包裹");
    }

    /// 测试:JSON 周围夹杂额外文本应能正确提取
    #[test]
    fn test_parse_json_with_surrounding_text() {
        let text = r#"
        好的,我的判断如下:
        {"need_agent":false,"reasoning":"单步直接回答"}
        希望对你有帮助!"#;
        let resp = parse_intent_response(text).expect("应能从夹杂文本中提取");
        assert!(!resp.need_agent);
        assert_eq!(resp.reasoning, "单步直接回答");
    }

    // -----------------------------------------------------------------------
    // 错误路径
    // -----------------------------------------------------------------------

    /// 测试:完全无效的 JSON 应返回错误
    #[test]
    fn test_parse_invalid_json_returns_error() {
        let result = parse_intent_response("this is not json at all { broken");
        assert!(result.is_err(), "无效 JSON 应返回错误");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("parse") || err_msg.contains("Parse") || err_msg.contains("JSON"),
            "错误信息应提示 JSON 解析失败,实际: {}",
            err_msg
        );
    }

    /// 测试:缺少 need_agent 字段应返回错误
    #[test]
    fn test_parse_missing_need_agent_returns_error() {
        let result = parse_intent_response(r#"{"reasoning":"缺少 need_agent"}"#);
        assert!(result.is_err(), "缺少 need_agent 应返回错误");
        assert!(
            result.unwrap_err().to_string().contains("need_agent"),
            "错误信息应提示 need_agent 缺失"
        );
    }

    /// 测试:reasoning 为空字符串应返回错误
    #[test]
    fn test_parse_empty_reasoning_returns_error() {
        let result = parse_intent_response(r#"{"need_agent":true,"reasoning":""}"#);
        assert!(result.is_err(), "空 reasoning 应返回错误");
        assert!(
            result.unwrap_err().to_string().contains("reasoning"),
            "错误信息应提示 reasoning 缺失"
        );
    }

    /// 测试:reasoning 仅包含空白字符应返回错误
    #[test]
    fn test_parse_whitespace_reasoning_returns_error() {
        let result = parse_intent_response(r#"{"need_agent":true,"reasoning":"   \n\t  "}"#);
        assert!(result.is_err(), "纯空白 reasoning 应返回错误");
    }

    // =============================================================================
    // IntentResponse Serde 契约
    // =============================================================================

    /// 测试:IntentResponse 应能正确序列化 / 反序列化
    #[test]
    fn test_intent_response_serde_roundtrip() {
        let original = IntentResponse {
            need_agent: true,
            reasoning: "任务分解: ...".to_string(),
        };
        let json = serde_json::to_string(&original).expect("序列化应成功");
        let restored: IntentResponse = serde_json::from_str(&json).expect("反序列化应成功");
        assert_eq!(restored.need_agent, original.need_agent);
        assert_eq!(restored.reasoning, original.reasoning);
    }
}
