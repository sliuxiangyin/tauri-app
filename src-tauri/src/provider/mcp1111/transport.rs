use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;

use super::error::McpError;
use super::types::{ToolCallRequest, ToolCallResult, ToolInfo};

/// 传输层trait
#[async_trait]
pub trait Transport: Send + Sync {
    async fn connect(&mut self) -> Result<(), McpError>;
    async fn disconnect(&mut self) -> Result<(), McpError>;
    async fn send_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, McpError>;
    async fn is_connected(&self) -> bool;
}

/// Stdio 传输
pub struct StdioTransport {
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    connected: Arc<RwLock<bool>>,
}

impl StdioTransport {
    pub fn new(command: String, args: Vec<String>, env: HashMap<String, String>) -> Self {
        Self {
            command,
            args,
            env,
            connected: Arc::new(RwLock::new(false)),
        }
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn connect(&mut self) -> Result<(), McpError> {
        // Stdio transport 在发送请求时动态启动进程
        let mut connected = self.connected.write().await;
        *connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), McpError> {
        let mut connected = self.connected.write().await;
        *connected = false;
        Ok(())
    }

    async fn send_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });

        let mut child = Command::new(&self.command)
            .args(&self.args)
            .envs(&self.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| McpError::Io(e))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::CommunicationError("Failed to open stdin".to_string()))?;
        stdin
            .write_all(request.to_string().as_bytes())
            .await
            .map_err(|e| McpError::Io(e))?;
        stdin.write_all(b"\n").await.map_err(|e| McpError::Io(e))?;
        drop(stdin);

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::CommunicationError("Failed to open stdout".to_string()))?;
        let mut reader = BufReader::new(stdout);
        let mut response_str = String::new();
        reader
            .read_line(&mut response_str)
            .await
            .map_err(|e| McpError::Io(e))?;
        let response: serde_json::Value =
            serde_json::from_str(&response_str).map_err(|e| McpError::Json(e))?;

        Ok(response)
    }

    async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }
}

/// HTTP 传输
pub struct HttpTransport {
    url: String,
    client: reqwest::Client,
    connected: Arc<RwLock<bool>>,
}

impl HttpTransport {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: reqwest::Client::new(),
            connected: Arc::new(RwLock::new(false)),
        }
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn connect(&mut self) -> Result<(), McpError> {
        // 先尝试一个简单的初始化请求
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });

        self.client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| McpError::ConnectionError(e.to_string()))?;

        let mut connected = self.connected.write().await;
        *connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), McpError> {
        let mut connected = self.connected.write().await;
        *connected = false;
        Ok(())
    }

    async fn send_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| McpError::CommunicationError(e.to_string()))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| McpError::CommunicationError(e.to_string()))?;

        Ok(response)
    }

    async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }
}
