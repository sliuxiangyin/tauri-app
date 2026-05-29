//! 意图分析提示词模块
//!
//! 提供意图识别的公共提示词和解析函数，供所有 Provider 复用

use crate::provider::llm::error::LlmError;
use crate::provider::llm::types::{ChatMessage, IntentPlan, PlanStep, Role, ToolDefinition};
use serde_json::Value;

/// 意图分析系统提示词
pub fn intent_system_prompt() -> &'static str {
    r#"你是一个意图分析助手。根据用户消息判断是否需要执行工具来完成请求。

【分析规则】
1. 如果用户请求需要搜索、查询、分析、比较等多步操作，设置 need_agent=true
2. 如果只是简单问答（如打招呼、询问事实），设置 need_agent=false
3. steps 中的 tool_name 必须使用完整格式 "mcp__server__tool"
4. 确保步骤之间有合理的依赖关系

【步骤类型判断】
- deterministic: 工具和参数在计划阶段已知，可以直接执行（如搜索确定的关键词）
- exploratory: 需要在执行时根据上下文决定工具和参数（如选择器、文件名、动态数据）

【判断标准】
- 如果步骤依赖前置步骤的输出（如选择器、文件名），设为 exploratory
- 如果用户请求模糊（如"帮我找个"、"选择最合适的"），设为 exploratory
- 如果工具参数在计划阶段可以确定，设为 deterministic

【输出格式】
请以 JSON 格式返回分析结果：
{
    "need_agent": true/false,
    "reasoning": "判断理由（1-2句话）",
    "steps": [
        {
            "order": 1,
            "step_type": "deterministic" | "exploratory",
            "tool_name": "工具名称（exploratory 时可为 null）",
            "parameters": {"参数名": "参数值"},
            "step_goal": "本步骤要完成的目标",
            "expected_output": "期望的结果（可选）",
            "depends_on": null
        }
    ]
}

【重要约束】
- order: 必须是数字，不要用方括号包裹
- step_type: 必须是 "deterministic" 或 "exploratory"
- depends_on: 必须是数字或 null，不要用方括号包裹
- parameters: 必须是 JSON 对象
  - deterministic: 必须包含完整参数
  - exploratory: 可为空对象 {}，工具和参数将在执行时由 LLM 决定
- 不要生成多余的逗号

注意：如果 need_agent=false，steps 应为空数组。"#
}

/// 从可用工具生成描述文本（包含参数信息）
pub fn build_tools_description(tools: &[ToolDefinition]) -> String {
    if tools.is_empty() {
        return "（无可用工具）".to_string();
    }
    tools.iter()
        .map(|t| {
            let params = extract_params_info(&t.function.parameters);
            if params.is_empty() {
                format!("- {}: {}",
                    t.function.name,
                    t.function.description.as_deref().unwrap_or("无描述")
                )
            } else {
                format!("- {}: {}\n  参数: {}",
                    t.function.name,
                    t.function.description.as_deref().unwrap_or("无描述"),
                    params
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 从 JSON Schema 提取参数信息
fn extract_params_info(parameters: &serde_json::Value) -> String {
    // 尝试解析 properties
    if let Some(props) = parameters.get("properties").and_then(|p| p.as_object()) {
        if props.is_empty() {
            return String::new();
        }
        let infos: Vec<String> = props.iter()
            .map(|(name, schema)| {
                let desc = schema.get("description").and_then(|d| d.as_str());
                let typ = schema.get("type").and_then(|t| t.as_str());
                if let Some(d) = desc {
                    format!("{}: {} ({})", name, d, typ.unwrap_or("any"))
                } else if let Some(t) = typ {
                    format!("{}: {}", name, t)
                } else {
                    name.clone()
                }
            })
            .collect();
        infos.join(", ")
    } else {
        String::new()
    }
}

/// 从消息历史提取用户最后一条请求
pub fn extract_user_request(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .filter(|m| m.role == Role::User)
        .last()
        .map(|m| m.content.clone())
        .unwrap_or_default()
}

/// 构建意图分析的用户消息
/// 当前 prompt 设计就是让 LLM 判断用哪个工具，不是让它调用工具 所以需要转换
pub fn build_intent_user_message(
    available_tools: &[ToolDefinition],
    user_request: &str,
) -> String {
    let tools_desc = build_tools_description(available_tools);
    format!(
        "【可用工具】\n{}\n\n【用户请求】\n{}",
        tools_desc,
        user_request
    )
}

/// 从 Value 中提取 u8，处理可能的数组格式
fn extract_u8(value: &Value, field_name: &str) -> Result<u8, String> {
    match value {
        Value::Number(n) => n.as_u64()
            .and_then(|v| if v > 0 && v <= 255 { Some(v as u8) } else { None })
            .ok_or_else(|| format!("{} 必须是 1-255 的数字", field_name)),
        Value::Array(arr) if !arr.is_empty() => {
            // 容错：如果 LLM 返回了数组 [1]，取第一个元素
            extract_u8(&arr[0], field_name)
        }
        _ => Err(format!("{} 必须是数字或 null", field_name)),
    }
}

/// 从 Value 中提取 Option<u8>，处理可能的数组格式
fn extract_optional_u8(value: &Value, field_name: &str) -> Result<Option<u8>, String> {
    match value {
        Value::Null => Ok(None),
        Value::Number(_) => extract_u8(value, field_name).map(Some),
        Value::Array(arr) if arr.is_empty() => Ok(None),
        Value::Array(arr) if !arr.is_empty() => {
            // 容错：如果 LLM 返回了数组 [1]，取第一个元素
            extract_u8(&arr[0], field_name).map(Some)
        }
        _ => Err(format!("{} 必须是数字或 null", field_name)),
    }
}

/// 解析 LLM 返回的意图计划 JSON
///
/// 能处理：
/// - 纯 JSON 字符串
/// - Markdown 代码块包裹的 JSON
/// - JSON 中包含额外文本
/// - LLM 错误地将 u8 字段输出为数组的情况（如 order: [1]）
pub fn parse_intent_response(response: &str) -> Result<IntentPlan, LlmError> {
    // 尝试提取 JSON（可能有 markdown 格式）
    let json_str = if response.contains('{') {
        let start = response.find('{').unwrap();
        let end = response.rfind('}').unwrap_or(response.len() - 1);
        &response[start..=end]
    } else {
        response
    };

    // 先解析为 serde_json::Value
    let json: Value = serde_json::from_str(json_str).map_err(|e| {
        LlmError::ParseError(format!("Failed to parse JSON: {}", e))
    })?;

    let obj = json.as_object().ok_or_else(|| {
        LlmError::ParseError("Invalid JSON structure: expected object".into())
    })?;

    let need_agent = obj.get("need_agent")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| LlmError::ParseError("need_agent must be a boolean".into()))?;

    let reasoning = obj.get("reasoning")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_default();

    let steps: Vec<PlanStep> = obj.get("steps")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter().enumerate().map(|(idx, step)| {
                let step_obj = step.as_object().ok_or_else(|| {
                    format!("steps[{}] must be an object", idx)
                });

                step_obj.and_then(|s| {
                    let order = extract_u8(
                        s.get("order").ok_or("order is required")?,
                        "order"
                    ).map_err(String::from)?;

                    // step_type: 先解析
                    let step_type = s.get("step_type")
                        .and_then(|v| v.as_str())
                        .map(|s| match s {
                            "exploratory" => crate::provider::llm::types::StepType::Exploratory,
                            _ => crate::provider::llm::types::StepType::Deterministic,
                        })
                        .unwrap_or(crate::provider::llm::types::StepType::Deterministic);
                    // tool_name: exploratory 可为 null，deterministic 必须有值
                    let tool_name = match step_type {
                        crate::provider::llm::types::StepType::Exploratory => {
                            s.get("tool_name")
                                .and_then(|v| v.as_str())
                                .map(String::from)
                                .unwrap_or_default()
                        }
                        _ => {
                            s.get("tool_name")
                                .and_then(|v| v.as_str())
                                .map(String::from)
                                .ok_or_else(|| "tool_name is required and must be a string".to_string())?
                        }
                    };

                    let step_goal = s.get("step_goal")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_default();

                    let expected_output = s.get("expected_output")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    let depends_on = s.get("depends_on")
                        .map(|v| extract_optional_u8(v, "depends_on"))
                        .transpose()
                        .map_err(String::from)?
                        .unwrap_or(None);

                    // parameters: 必须是 JSON 对象
                    let parameters = s.get("parameters")
                        .map(|v| {
                            if v.is_object() {
                                Ok(v.clone())
                            } else if v.is_null() {
                                Ok(serde_json::json!({}))
                            } else {
                                Err("parameters must be a JSON object".to_string())
                            }
                        })
                        .transpose()
                        .map_err(String::from)?
                        .unwrap_or_else(|| serde_json::json!({}));

                    Ok(PlanStep {
                        order,
                        step_type,
                        tool_name,
                        parameters,
                        step_goal,
                        expected_output,
                        depends_on,
                    })
                })
            }).collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(|e| LlmError::ParseError(format!("Failed to parse steps: {}", e)))?
        .unwrap_or_default();

    Ok(IntentPlan {
        need_agent,
        reasoning,
        steps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_system_prompt_contains_rules() {
        let prompt = intent_system_prompt();
        assert!(prompt.contains("need_agent"));
        assert!(prompt.contains("mcp__server__tool"));
        assert!(prompt.contains("steps"));
    }

    #[test]
    fn test_build_tools_description_empty() {
        let tools: Vec<ToolDefinition> = vec![];
        let desc = build_tools_description(&tools);
        assert_eq!(desc, "（无可用工具）");
    }

    #[test]
    fn test_extract_user_request() {
        let messages = vec![
            ChatMessage::new(Role::System, "你是一个助手"),
            ChatMessage::new(Role::User, "你好"),
            ChatMessage::new(Role::Assistant, "你好！"),
            ChatMessage::new(Role::User, "帮我搜索天气"),
        ];
        let request = extract_user_request(&messages);
        assert_eq!(request, "帮我搜索天气");
    }

    #[test]
    fn test_parse_intent_response_simple() {
        let json = r#"{"need_agent":false,"reasoning":"简单问答","steps":[]}"#;
        let plan = parse_intent_response(json).unwrap();
        assert!(!plan.need_agent);
        assert_eq!(plan.steps.len(), 0);
    }

    #[test]
    fn test_parse_intent_response_with_markdown() {
        let json = r#"```json
{"need_agent":true,"reasoning":"需要多步操作","steps":[{"order":1,"tool_name":"mcp__test__tool","parameters":{"query":"test"},"step_goal":"执行操作","depends_on":null}]}
```"#;
        let plan = parse_intent_response(json).unwrap();
        assert!(plan.need_agent);
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].parameters, serde_json::json!({"query": "test"}));
    }

    #[test]
    fn test_parse_intent_response_with_parameters_array_fallback() {
        // 测试 parameters 字段被误写为数组时的容错
        let json = r#"{"need_agent":true,"reasoning":"测试","steps":[{"order":1,"tool_name":"mcp__test__tool","parameters":[{"key":"value"}],"step_goal":"测试","depends_on":null}]}"#;
        let plan = parse_intent_response(json).unwrap();
        assert!(plan.need_agent);
        // 参数为数组时会变成空对象
        assert_eq!(plan.steps[0].parameters, serde_json::json!({}));
    }
}