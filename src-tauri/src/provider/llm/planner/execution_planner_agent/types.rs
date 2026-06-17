//! Execution Planner Agent - 类型定义
//!
//! 定义 Execution Step 与 Execution Plan 的结构。
//! 遵循 ARCHITECTURE.md 第十二节"类型定义"。
//!
//! Execution Plan 是单个 Task Stage 的执行细化：
//! - 每个 Step 描述一个可被 React Agent 执行的原子操作
//! - Step 之间通过 `depends_on` 形成 DAG（支持并行执行）
//! - `expected_tool_category` 指向工具类别，而非具体工具名

use serde::{Deserialize, Serialize};

/// Execution Step：完成一个 Stage 所需的执行步骤。
///
/// 面向执行（细粒度），包含依赖关系，可映射到工具类别。
/// 具体工具选择和参数构造留给 React Agent。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionStep {
    /// 步骤序号（从 1 开始）
    pub order: u32,
    /// 步骤目标（执行语义，如"定位搜索输入框"）
    pub goal: String,
    /// 依赖的前置 Step 序号列表
    ///
    /// 无依赖的 Step 可并行执行，`depends_on` 为空数组。
    #[serde(default)]
    pub depends_on: Vec<u32>,
    /// 预期使用的工具类别（如 `browser_interaction`、`fs_read`、`llm_reasoning`）
    ///
    /// 填写工具类别而非具体工具名，具体工具由 React Agent 选择。
    /// `analysis` 领域使用内置类别 `llm_reasoning`（无需外部工具，LLM 直接推理输出）。
    pub expected_tool_category: String,
}

/// Execution Plan：一组 Execution Step 形成的 DAG。
///
/// 对应单个 Task Stage 的执行细化。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionPlan {
    /// 执行步骤列表
    pub steps: Vec<ExecutionStep>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_step_serialization_uses_snake_case() {
        let step = ExecutionStep {
            order: 1,
            goal: "定位搜索输入框".to_string(),
            depends_on: vec![],
            expected_tool_category: "browser_interaction".to_string(),
        };

        let json = serde_json::to_string(&step).expect("serialize ok");
        assert!(json.contains("\"order\":1"));
        assert!(json.contains("\"goal\":\"定位搜索输入框\""));
        assert!(json.contains("\"expected_tool_category\":\"browser_interaction\""));
    }

    #[test]
    fn execution_step_deserializes_from_json() {
        let json = r#"{
            "order": 2,
            "goal": "在搜索输入框中输入关键词",
            "depends_on": [1],
            "expected_tool_category": "browser_input"
        }"#;

        let step: ExecutionStep = serde_json::from_str(json).expect("deserialize ok");
        assert_eq!(step.order, 2);
        assert_eq!(step.depends_on, vec![1]);
        assert_eq!(step.expected_tool_category, "browser_input");
    }

    #[test]
    fn execution_step_depends_on_defaults_to_empty() {
        let json = r#"{
            "order": 1,
            "goal": "确认页面已加载",
            "expected_tool_category": "browser_wait"
        }"#;

        let step: ExecutionStep = serde_json::from_str(json).expect("deserialize ok");
        assert!(step.depends_on.is_empty());
    }

    #[test]
    fn execution_plan_roundtrip() {
        let plan = ExecutionPlan {
            steps: vec![
                ExecutionStep {
                    order: 1,
                    goal: "确认页面已加载".to_string(),
                    depends_on: vec![],
                    expected_tool_category: "browser_wait".to_string(),
                },
                ExecutionStep {
                    order: 2,
                    goal: "定位搜索输入框".to_string(),
                    depends_on: vec![1],
                    expected_tool_category: "browser_interaction".to_string(),
                },
                ExecutionStep {
                    order: 3,
                    goal: "输入关键词".to_string(),
                    depends_on: vec![2],
                    expected_tool_category: "browser_input".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&plan).expect("serialize ok");
        let decoded: ExecutionPlan = serde_json::from_str(&json).expect("deserialize ok");
        assert_eq!(decoded.steps.len(), 3);
        assert_eq!(decoded.steps[1].depends_on, vec![1]);
        assert_eq!(decoded.steps[2].depends_on, vec![2]);
    }

    #[test]
    fn execution_plan_deserializes_from_llm_output() {
        let json = r#"{
            "steps": [
                {
                    "order": 1,
                    "goal": "确认文件存在",
                    "depends_on": [],
                    "expected_tool_category": "fs_read"
                },
                {
                    "order": 2,
                    "goal": "读取文件内容",
                    "depends_on": [1],
                    "expected_tool_category": "fs_read"
                },
                {
                    "order": 3,
                    "goal": "解析 JSON 对象",
                    "depends_on": [2],
                    "expected_tool_category": "fs_parse"
                }
            ]
        }"#;

        let plan: ExecutionPlan = serde_json::from_str(json).expect("deserialize ok");
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[0].order, 1);
        assert!(plan.steps[0].depends_on.is_empty());
        assert_eq!(plan.steps[2].expected_tool_category, "fs_parse");
    }
}
