use rmcp::service::{RoleClient, RunningService};
use tracing::info;

use crate::provider::mcp_v2::config::TransportConfig;
use crate::provider::mcp_v2::error::{McpManagerError, Result};

/// 根据传输配置建立 MCP 连接，返回 RunningService 句柄
pub async fn build_peer(config: &TransportConfig) -> Result<RunningService<RoleClient, ()>> {
    match config {
        TransportConfig::Stdio { command, args } => {
            let mut cmd = tokio::process::Command::new(command);
            cmd.args(args);
            // 将子进程 stderr 重定向到 null，避免 MCP 服务器内部错误污染主进程输出
            cmd.stderr(std::process::Stdio::null());
            let transport =
                rmcp::transport::child_process::TokioChildProcess::new(cmd).map_err(|e| {
                    McpManagerError::TransportError {
                        message: format!("Failed to create stdio transport: {}", e),
                    }
                })?;
            let client = rmcp::serve_client((), transport).await.map_err(|e| {
                McpManagerError::ConnectionFailed {
                    message: format!("Failed to serve stdio client: {}", e),
                }
            })?;
            info!("STDIO MCP connection established for command: {}", command);
            Ok(client)
        }
        TransportConfig::Http { url } => {
            let transport = rmcp::transport::StreamableHttpClientTransport::from_uri(url.clone());
            let client = rmcp::serve_client((), transport).await.map_err(|e| {
                McpManagerError::ConnectionFailed {
                    message: format!("Failed to serve HTTP client for '{}': {}", url, e),
                }
            })?;
            info!("HTTP MCP connection established for URL: {}", url);
            Ok(client)
        }
    }
}
