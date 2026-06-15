//! 工具相关函数
//!
//! 提供 MCP 工具名称的解析与构建功能。
//!
//! ## 工具命名规范
//! MCP 工具统一使用 `{prefix}__{server}__{tool}` 格式，例如：
//! - `mcp__browser__navigate`
//! - `mcp__fs__read_file`

/// 解析 MCP 工具名称
///
/// 从完整工具名中提取 server 名和 tool 名。
/// 标准格式: `"mcp__server__tool_name"`
///
/// # 示例
/// ```
/// use tauri_app::provider::llm::tools::parse_mcp_tool_name;
///
/// assert_eq!(
///     parse_mcp_tool_name("mcp__playwright__goto"),
///     Some(("playwright", "goto"))
/// );
/// assert_eq!(parse_mcp_tool_name("playwright__goto"), None);
/// ```
pub fn parse_mcp_tool_name(full_name: &str) -> Option<(&str, &str)> {
    // 期望格式: "mcp__server__tool"
    if let Some(rest) = full_name.strip_prefix("mcp__") {
        let parts: Vec<&str> = rest.split("__").collect();
        if parts.len() == 2 {
            return Some((parts[0], parts[1]));
        }
    }
    None
}

/// 构建 MCP 工具名称
///
/// 将 server 名和 tool 名拼接为标准格式 `"mcp__server__tool"`。
///
/// # 示例
/// ```
/// use tauri_app::provider::llm::tools::build_mcp_tool_name;
///
/// assert_eq!(
///     build_mcp_tool_name("playwright", "goto"),
///     "mcp__playwright__goto"
/// );
/// ```
pub fn build_mcp_tool_name(server_name: &str, tool_name: &str) -> String {
    format!("mcp__{}__{}", server_name, tool_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mcp_tool_name() {
        assert_eq!(
            parse_mcp_tool_name("mcp__playwright__goto"),
            Some(("playwright", "goto"))
        );
        assert_eq!(
            parse_mcp_tool_name("mcp__browser__navigate"),
            Some(("browser", "navigate"))
        );
        assert_eq!(parse_mcp_tool_name("playwright__goto"), None);
        assert_eq!(parse_mcp_tool_name("invalid"), None);
    }

    #[test]
    fn test_parse_mcp_tool_name_multi_underscore() {
        // server 或 tool 名中包含多个 __ 时应返回 None（只接受严格两段）
        assert_eq!(parse_mcp_tool_name("mcp__a__b__c"), None);
    }

    #[test]
    fn test_build_mcp_tool_name() {
        assert_eq!(
            build_mcp_tool_name("playwright", "goto"),
            "mcp__playwright__goto"
        );
        assert_eq!(
            build_mcp_tool_name("fs", "read_file"),
            "mcp__fs__read_file"
        );
    }
}
