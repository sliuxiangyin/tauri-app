use super::error::McpError;
use super::transport::Transport;
use super::types::{CachedToolsList, McpModelConfig, ToolCallResult, ToolInfo};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct McpClient {
    transport: Arc<RwLock<Box<dyn Transport>>>,
    tools_cache: Arc<RwLock<Option<CachedToolsList>>>,
    retry_count: u32,
}

impl McpClient {
    pub fn new(config: McpModelConfig) -> Result<Self, McpError> {
        let transport: Box<dyn Transport> = match config {
            McpModelConfig::Stdio { command, args, env } => {
                Box::new(super::transport::StdioTransport::new(command, args, env))
            }
            McpModelConfig::Http { url } => Box::new(super::transport::HttpTransport::new(url)),
        };

        Ok(Self {
            transport: Arc::new(RwLock::new(transport)),
            tools_cache: Arc::new(RwLock::new(None)),
            retry_count: 3,
        })
    }

    /// 带重试的连接
    pub async fn connect_with_retry(&self) -> Result<(), McpError> {
        let mut last_error = McpError::ConnectionFailedAfterRetries;

        for attempt in 0..self.retry_count {
            let mut transport = self.transport.write().await;
            match transport.connect().await {
                Ok(_) => {
                    println!("MCP connected on attempt {}", attempt + 1);
                    return Ok(());
                }
                Err(e) => {
                    println!("MCP connection attempt {} failed: {}", attempt + 1, e);
                    last_error = e;
                    drop(transport);

                    if attempt < self.retry_count - 1 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }

        Err(last_error)
    }

    pub async fn disconnect(&self) -> Result<(), McpError> {
        let mut transport = self.transport.write().await;
        transport.disconnect().await
    }

    pub async fn is_connected(&self) -> bool {
        let transport = self.transport.read().await;
        transport.is_connected().await
    }

    /// 获取工具列表
    pub async fn list_tools(&self, force_refresh: bool) -> Result<Vec<ToolInfo>, McpError> {
        // 检查缓存
        if !force_refresh {
            if let Some(cached) = self.tools_cache.read().await.as_ref() {
                if !cached.is_expired() {
                    return Ok(cached.tools.clone());
                }
            }
        }
        // 调用 MCP 服务的 list_tools
        let transport = self.transport.read().await;
        let response = transport.send_request("tools/list", json!({})).await?;
        println!("MCP list_tools response: {:?}", response);
        // 解析响应
        let tools = self.parse_tools_response(&response)?;
        println!("MCP tools response: {:?}", tools);
        // 缓存结果（TTL 5分钟）
        let cached = CachedToolsList::new(tools.clone(), 300);
        *self.tools_cache.write().await = Some(cached);

        Ok(tools)
    }

    fn parse_tools_response(
        &self,
        response: &serde_json::Value,
    ) -> Result<Vec<ToolInfo>, McpError> {
        let result = response
            .get("result")
            .ok_or_else(|| McpError::ProtocolError("Invalid tools response format".to_string()))?;

        // 检查 result 是否包含 tools 字段
        let tools = result
            .get("tools")
            .ok_or_else(|| McpError::ProtocolError("Tools not found in response".to_string()))?
            .as_array()
            .ok_or_else(|| McpError::ProtocolError("Tools should be an array".to_string()))?;

        let tools = tools
            .iter()
            .filter_map(|tool| {
                let name = tool.get("name")?.as_str()?.to_string();
                let description = tool
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string());
                let input_schema = tool.get("inputSchema").cloned().unwrap_or(json!({}));
                Some(ToolInfo {
                    name,
                    description,
                    input_schema,
                })
            })
            .collect();

        Ok(tools)
    }

    /// 调用工具
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolCallResult, McpError> {
        let transport = self.transport.read().await;
        let params = json!({
            "name": tool_name,
            "arguments": arguments
        });

        let response: serde_json::Value = transport.send_request("tools/call", params).await?;

        // 解析响应
        let content = response
            .get("content")
            .ok_or_else(|| McpError::ToolExecutionError("No content in response".to_string()))?
            .as_array()
            .ok_or_else(|| McpError::ToolExecutionError("Content should be an array".to_string()))?
            .iter()
            .filter_map(|item| {
                let r#type = item.get("type")?.as_str()?.to_string();
                let text = item
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());
                Some(super::types::ToolContent {
                    r#type,
                    text,
                    data: None,
                    mime_type: None,
                })
            })
            .collect();

        let is_error = response
            .get("isError")
            .and_then(|e| e.as_bool())
            .unwrap_or(false);

        Ok(ToolCallResult { content, is_error })
    }

    /// 清除工具缓存
    pub async fn clear_tools_cache(&self) {
        *self.tools_cache.write().await = None;
    }
}
