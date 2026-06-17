//! Task Planner Agent - 类型定义
//!
//! 定义 Task Stage 与 Task Plan 的结构。
//! 遵循 ARCHITECTURE.md 第十四节"类型定义"。
//!
//! Stage 之间的数据流通过 `inputs` / `outputs` 显式声明：
//! - `outputs` 描述当前 Stage 产出哪些可被引用的数据
//! - `inputs` 描述当前 Stage 消费哪些数据，来源为字面量或前置 Stage 的输出
//! - Execution Planner 在执行时按 DAG 注入 `FromStage` 类型的实际值

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Task Stage：完成任务必须经历的业务阶段。
///
/// 面向业务目标（不关心实现），属于同一个领域，支持 DAG 依赖编排。
/// 通过 `inputs` / `outputs` 显式声明数据流，支撑 Stage 间的参数传递。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskStage {
    /// 阶段唯一标识
    pub id: String,
    /// 阶段目标（业务语义）
    pub goal: String,
    /// 所属领域
    /// （browser | file | adb | office | database | http | terminal）
    pub domain: String,
    /// 依赖的前置 Stage ID 列表（DAG）
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// 当前 Stage 产生的输出（key: 输出名, value: 输出规约）
    /// 供其他 Stage 通过 `InputSource::FromStage` 引用
    #[serde(default)]
    pub outputs: BTreeMap<String, OutputSpec>,
    /// 当前 Stage 需要的输入（key: 输入名, value: 输入规约）
    /// 值为字面量或对前置 Stage 输出的引用
    #[serde(default)]
    pub inputs: BTreeMap<String, InputSpec>,
}

/// Task Plan：一组 Task Stage 形成的 DAG。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskPlan {
    /// 任务阶段列表
    pub stages: Vec<TaskStage>,
}

/// Stage 输出规约：描述当前 Stage 产出的可被引用的数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OutputSpec {
    /// 输出描述（如 "搜索结果列表"、"读取的文件路径"）
    pub description: String,
    /// 数据类型提示
    /// （"string" | "number" | "boolean" | "list" | "object" | "file_path" | "url" | ...）
    pub r#type: String,
}

/// Stage 输入规约：描述当前 Stage 消费的参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputSpec {
    /// 输入参数描述
    pub description: String,
    /// 数据类型提示（与 `OutputSpec::type` 同义）
    pub r#type: String,
    /// 值的来源：字面量常量 或 引用前置 Stage 的输出
    pub source: InputSource,
}

/// 输入值的来源。
///
/// 使用 serde tag = "kind" 形成 JSON 判别字段，便于 LLM 输出稳定结构：
/// ```json
/// { "kind": "literal", "value": "https://example.com" }
/// { "kind": "from_stage", "stage_id": "stage-1", "output_name": "url" }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum InputSource {
    /// 字面量常量：值在规划时已确定（如 URL、关键词、文件路径）
    Literal {
        /// 字面量值，使用 `serde_json::Value` 支持字符串/数字/对象/数组等
        value: serde_json::Value,
    },
    /// 引用前置 Stage 的输出：执行时由 Execution Planner 按 DAG 注入
    FromStage {
        /// 前置 Stage 的 ID
        stage_id: String,
        /// 该 Stage 输出的 `OutputSpec.name`
        output_name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_stage_serialization_uses_snake_case() {
        let stage = TaskStage {
            id: "stage-1".to_string(),
            goal: "打开浏览器并导航到搜索页".to_string(),
            domain: "browser".to_string(),
            depends_on: vec![],
            outputs: BTreeMap::new(),
            inputs: BTreeMap::new(),
        };

        let json = serde_json::to_string(&stage).expect("serialize ok");
        assert!(json.contains("\"id\":\"stage-1\""));
        assert!(json.contains("\"goal\":\"打开浏览器并导航到搜索页\""));
        assert!(json.contains("\"domain\":\"browser\""));
    }

    #[test]
    fn task_stage_deserializes_from_json() {
        let json = r#"{
            "id": "stage-2",
            "goal": "输入关键词执行搜索",
            "domain": "browser",
            "depends_on": ["stage-1"]
        }"#;

        let stage: TaskStage = serde_json::from_str(json).expect("deserialize ok");
        assert_eq!(stage.id, "stage-2");
        assert_eq!(stage.domain, "browser");
        assert_eq!(stage.depends_on, vec!["stage-1".to_string()]);
    }

    #[test]
    fn task_stage_depends_on_defaults_to_empty() {
        let json = r#"{
            "id": "stage-1",
            "goal": "读取配置文件",
            "domain": "file"
        }"#;

        let stage: TaskStage = serde_json::from_str(json).expect("deserialize ok");
        assert!(stage.depends_on.is_empty());
    }

    #[test]
    fn task_plan_roundtrip() {
        let plan = TaskPlan {
            stages: vec![
                TaskStage {
                    id: "stage-1".to_string(),
                    goal: "打开浏览器".to_string(),
                    domain: "browser".to_string(),
                    depends_on: vec![],
                    outputs: BTreeMap::new(),
                    inputs: BTreeMap::new(),
                },
                TaskStage {
                    id: "stage-2".to_string(),
                    goal: "执行搜索".to_string(),
                    domain: "browser".to_string(),
                    depends_on: vec!["stage-1".to_string()],
                    outputs: BTreeMap::new(),
                    inputs: BTreeMap::new(),
                },
            ],
        };

        let json = serde_json::to_string(&plan).expect("serialize ok");
        let decoded: TaskPlan = serde_json::from_str(&json).expect("deserialize ok");
        assert_eq!(decoded.stages.len(), 2);
        assert_eq!(decoded.stages[1].depends_on, vec!["stage-1".to_string()]);
    }

    #[test]
    fn input_source_literal_serializes_with_tag() {
        let input = InputSpec {
            description: "搜索关键词".to_string(),
            r#type: "string".to_string(),
            source: InputSource::Literal {
                value: serde_json::json!("AI 新闻"),
            },
        };

        let json = serde_json::to_string(&input).expect("serialize ok");
        assert!(json.contains("\"kind\":\"literal\""), "actual: {json}");
        assert!(json.contains("\"value\":\"AI 新闻\""), "actual: {json}");
    }

    #[test]
    fn input_source_from_stage_roundtrip() {
        let input = InputSpec {
            description: "搜索页 URL".to_string(),
            r#type: "url".to_string(),
            source: InputSource::FromStage {
                stage_id: "stage-1".to_string(),
                output_name: "search_url".to_string(),
            },
        };

        let json = serde_json::to_string(&input).expect("serialize ok");
        assert!(json.contains("\"kind\":\"from_stage\""), "actual: {json}");
        assert!(json.contains("\"stage_id\":\"stage-1\""), "actual: {json}");
        assert!(json.contains("\"output_name\":\"search_url\""), "actual: {json}");

        let decoded: InputSpec = serde_json::from_str(&json).expect("deserialize ok");
        match decoded.source {
            InputSource::FromStage { stage_id, output_name } => {
                assert_eq!(stage_id, "stage-1");
                assert_eq!(output_name, "search_url");
            }
            _ => panic!("expected FromStage variant"),
        }
    }

    #[test]
    fn task_stage_with_inputs_and_outputs() {
        let json = r#"{
            "id": "stage-search",
            "goal": "在搜索页输入关键词并提交",
            "domain": "browser",
            "depends_on": ["stage-open"],
            "outputs": {
                "results": {
                    "description": "前十条搜索结果",
                    "type": "list"
                }
            },
            "inputs": {
                "url": {
                    "description": "搜索页 URL",
                    "type": "url",
                    "source": { "kind": "literal", "value": "https://www.baidu.com" }
                },
                "keyword": {
                    "description": "搜索关键词",
                    "type": "string",
                    "source": {
                        "kind": "from_stage",
                        "stage_id": "stage-extract",
                        "output_name": "topic"
                    }
                }
            }
        }"#;

        let stage: TaskStage = serde_json::from_str(json).expect("deserialize ok");
        assert_eq!(stage.outputs.len(), 1);
        assert!(stage.outputs.contains_key("results"));
        assert_eq!(stage.inputs.len(), 2);

        let url = stage.inputs.get("url").expect("url input exists");
        match &url.source {
            InputSource::Literal { value } => {
                assert_eq!(value, &serde_json::json!("https://www.baidu.com"));
            }
            _ => panic!("url.source should be Literal"),
        }

        let keyword = stage.inputs.get("keyword").expect("keyword input exists");
        match &keyword.source {
            InputSource::FromStage { stage_id, output_name } => {
                assert_eq!(stage_id, "stage-extract");
                assert_eq!(output_name, "topic");
            }
            _ => panic!("keyword.source should be FromStage"),
        }
    }

    #[test]
    fn inputs_outputs_default_to_empty() {
        let json = r#"{
            "id": "stage-x",
            "goal": "x",
            "domain": "file"
        }"#;

        let stage: TaskStage = serde_json::from_str(json).expect("deserialize ok");
        assert!(stage.inputs.is_empty());
        assert!(stage.outputs.is_empty());
    }
}
