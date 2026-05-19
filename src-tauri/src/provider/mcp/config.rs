use serde::{Deserialize, Serialize};

/// MCP 服务器配置
/// 注意：此配置不涉及数据库操作，由外部（Tauri 层）负责持久化
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// 服务器唯一标识
    pub id: String,
    /// 服务器名称（用于显示）
    pub name: String,
    /// 服务器描述（可选）
    #[serde(default)]
    pub description: Option<String>,
    /// 传输配置
    pub transport: TransportConfig,
}

impl McpServerConfig {
    /// 创建新的 STDIO 类型服务器配置
    pub fn new_stdio(id: impl Into<String>, name: impl Into<String>, command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            transport: TransportConfig::Stdio {
                command: command.into(),
                args,
            },
        }
    }

    /// 创建新的 HTTP 类型服务器配置
    pub fn new_http(id: impl Into<String>, name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            transport: TransportConfig::Http {
                url: url.into(),
            },
        }
    }

    /// 检查是否为 STDIO 类型
    pub fn is_stdio(&self) -> bool {
        matches!(self.transport, TransportConfig::Stdio { .. })
    }

    /// 检查是否为 HTTP 类型
    pub fn is_http(&self) -> bool {
        matches!(self.transport, TransportConfig::Http { .. })
    }

    /// 获取 STDIO 命令（仅 STDIO 类型有效）
    pub fn get_stdio_command(&self) -> Option<&str> {
        match &self.transport {
            TransportConfig::Stdio { command, .. } => Some(command),
            _ => None,
        }
    }
}

/// 传输类型配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TransportConfig {
    /// STDIO 传输（通过子进程）
    #[serde(rename = "stdio")]
    Stdio {
        /// 命令（如 npx、node、python 等）
        command: String,
        /// 命令参数
        #[serde(default)]
        args: Vec<String>,
    },
    /// HTTP 传输（通过 HTTP 请求）
    #[serde(rename = "http")]
    Http {
        /// 服务器 URL
        url: String,
    },
}

impl Default for TransportConfig {
    fn default() -> Self {
        TransportConfig::Stdio {
            command: String::new(),
            args: Vec::new(),
        }
    }
}