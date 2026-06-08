//! 意图分析提示词模块
//!
//! 提供意图识别的公共提示词和解析函数，供所有 Provider 复用

use crate::provider::llm::error::LlmError;
use crate::provider::llm::types::{ChatMessage, IntentPlan, PlanStep, Role, ToolDefinition};
use serde_json::Value;

/// 意图分析系统提示词
pub fn intent_system_prompt() -> &'static str {
    r#"你是一个意图分析助手。根据用户消息，判断是否需要通过多步工具组合来完成请求，并以 JSON 返回分析结果。

## 判断逻辑
- **need_agent = true**：请求需要搜索、查询、对比、分析、筛选等，且无法一步完成（例如：查A→对比B→筛选C）。
- **need_agent = false**：简单问答、问候、单步可完成的事实性查询（此时 steps 必须为 []）。

## 步骤类型
- **deterministic**：工具名和参数在规划时已完全确定（如搜索固定关键词）。
- **exploratory**：工具或参数须依赖前置步骤的输出（如文件选择器、动态ID、模糊请求"帮我找个合适的…"）。**此时 tool_name 必须为 null，parameters 必须为 {}，不得编造任何具体值。**

### 类型判定规则
满足以下任一条件即为 exploratory，否则为 deterministic：
1. 步骤依赖前置步骤的输出（如 depends_on 非空）。
2. 用户请求中未明确指定操作对象（如"选一个性价比高的"）。
3. 需要在上一步结果中动态提取参数（如文件名、URL、ID、CSS选择器）。
4. 某参数（如网页元素的 CSS 选择器、文件路径、动态生成的 ID）在当前阶段未知，绝对禁止猜测或编造。若无法确定，必须设为 exploratory，让后续执行时根据实际上下文获取。

## 输出格式
严格按以下 JSON 结构输出，不含注释、不添加多余字段：

{
  "need_agent": true,
  "reasoning": "简短判断理由（1-2句）",
  "steps": [
    {
      "order": 1,
      "step_type": "deterministic",
      "tool_name": "mcp__server__tool",
      "parameters": {"key": "value"},
      "step_goal": "本步目标",
      "expected_output": "期望的返回值（可选）",
      "depends_on": null
    }
  ]
}

## 硬性约束
- order 和 depends_on 必须是数字或 null，不能用方括号。
- step_type 只能是 "deterministic" 或 "exploratory"。
- deterministic 必须提供完整 tool_name 和 parameters；exploratory 的 tool_name 设为 null，parameters 设为 {}，绝不允许填写任何不确定的参数值。
- need_agent 为 false 时，steps 必须为 []。
- 不要输出任何 JSON 之外的文本，不要使用尾随逗号。

## 示例
用户消息："帮我查一下今天北京天气"
{
  "need_agent": false,
  "reasoning": "一步可直接查询天气，无需多步工具组合",
  "steps": []
}

用户消息："比较iPhone15和华为P60的拍照、价格，选拍照更好的"
{
  "need_agent": true,
  "reasoning": "需分别搜索两款手机参数，然后对比并筛选",
  "steps": [
    {
      "order": 1,
      "step_type": "deterministic",
      "tool_name": "mcp__search__web",
      "parameters": {"query": "iPhone15 拍照 价格 参数"},
      "step_goal": "获取iPhone15的拍照与价格信息",
      "expected_output": "包含参数和价格的文本",
      "depends_on": null
    },
    {
      "order": 2,
      "step_type": "deterministic",
      "tool_name": "mcp__search__web",
      "parameters": {"query": "华为P60 拍照 价格 参数"},
      "step_goal": "获取华为P60的拍照与价格信息",
      "expected_output": "包含参数和价格的文本",
      "depends_on": null
    },
    {
      "order": 3,
      "step_type": "exploratory",
      "tool_name": null,
      "parameters": {},
      "step_goal": "对比两部手机拍照并选出更优者",
      "expected_output": "选择结果及理由",
      "depends_on": 1
    }
  ]
}"#
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
