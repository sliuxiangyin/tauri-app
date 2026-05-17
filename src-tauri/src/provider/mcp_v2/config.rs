use serde::Deserialize;

/// 传输类型配置（支持 STDIO 和 HTTP）
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum TransportConfig {
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    #[serde(rename = "http")]
    Http { url: String },
}

/// 单个 MCP 服务器配置
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    /// 服务器唯一标识
    pub id: String,
    /// 服务器名称
    pub name: String,
    /// 传输配置
    pub transport: TransportConfig,
}

/// 全局 mcp-v2 配置
#[derive(Debug, Clone, Deserialize)]
pub struct McpV2Config {
    /// 缓存目录路径
    pub cache_dir: String,
    /// MCP 服务器列表
    pub servers: Vec<McpServerConfig>,
    /// 后台刷新间隔（秒），0 表示不启用
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: u64,
}

fn default_refresh_interval() -> u64 {
    0
}

impl Default for McpV2Config {
    fn default() -> Self {
        Self {
            cache_dir: "./mcp-v2-cache".to_string(),
            servers: Vec::new(),
            refresh_interval_secs: 0,
        }
    }
}
