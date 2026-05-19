use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{info, warn};

use crate::provider::mcp::error::{McpError, Result};

/// STDIO 进程管理器
/// 负责 STDIO 类型 MCP 服务器的进程生命周期管理
pub struct StdioProcessManager;

impl StdioProcessManager {
    /// 检查命令是否可用（通过执行 --version）
    pub async fn check_command(command: &str) -> Result<bool> {
        let mut cmd = Command::new(command);
        cmd.arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        match cmd.spawn() {
            Ok(mut child) => {
                match timeout(Duration::from_secs(5), child.wait()).await {
                    Ok(Ok(status)) => Ok(status.success()),
                    Ok(Err(e)) => {
                        warn!("Command '{}' check failed: {}", command, e);
                        Ok(false)
                    }
                    Err(_) => {
                        warn!("Command '{}' check timed out", command);
                        Ok(false)
                    }
                }
            }
            Err(e) => {
                warn!("Failed to spawn command '{}': {}", command, e);
                Ok(false)
            }
        }
    }

    /// 获取命令的版本信息
    pub async fn get_version(command: &str, args: &[String]) -> Result<String> {
        let mut cmd = Command::new(command);
        for arg in args.iter().take(2) {
            // 最多取前2个参数
            cmd.arg(arg);
        }
        cmd.arg("--version");
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());

        let output = cmd
            .spawn()
            .map_err(|e| McpError::ProcessError {
                message: format!("Failed to spawn '{}': {}", command, e),
            })?
            .wait_with_output()
            .await
            .map_err(|e| McpError::ProcessError {
                message: format!("Failed to wait for '{}': {}", command, e),
            })?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(version)
        } else {
            Err(McpError::ProcessError {
                message: format!("Command '{}' version check failed", command),
            })
        }
    }

    /// 创建 STDIO 传输
    pub async fn build_stdio_transport(
        command: &str,
        args: &[String],
    ) -> Result<rmcp::transport::child_process::TokioChildProcess> {
        let mut cmd = Command::new(command);
        cmd.args(args);
        // 将 stderr 重定向到 null，避免 MCP 服务器内部错误污染主进程输出
        cmd.stderr(Stdio::null());

        let transport = rmcp::transport::child_process::TokioChildProcess::new(cmd).map_err(|e| {
            McpError::TransportError {
                message: format!("Failed to create stdio transport for '{}': {}", command, e),
            }
        })?;

        Ok(transport)
    }

    /// 优雅关闭进程（先等待，超时则 kill）
    pub async fn graceful_shutdown(
        transport: &mut rmcp::transport::child_process::TokioChildProcess,
        timeout_duration: Duration,
    ) -> Result<()> {
        match timeout(timeout_duration, transport.graceful_shutdown()).await {
            Ok(Ok(())) => {
                info!("STDIO process shutdown gracefully");
                Ok(())
            }
            Ok(Err(e)) => {
                warn!("STDIO process shutdown error: {}", e);
                Err(McpError::ProcessError {
                    message: format!("Failed to shutdown process: {}", e),
                })
            }
            Err(_) => {
                warn!("STDIO process shutdown timed out, force kill");
                // 超时后进程会被 rmcp 自动清理
                Err(McpError::ProcessError {
                    message: "Process shutdown timed out".to_string(),
                })
            }
        }
    }

    /// 检查进程是否还在运行
    #[cfg(windows)]
    pub fn is_process_alive(process_id: u32) -> bool {
        use std::ffi::c_void;

        type Handle = *mut c_void;
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> Handle;
            fn CloseHandle(handle: Handle) -> i32;
            fn GetExitCodeProcess(handle: Handle, exit_code: *mut u32) -> i32;
        }

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
            if handle.is_null() {
                return false;
            }

            let mut exit_code: u32 = 0;
            let result = GetExitCodeProcess(handle, &mut exit_code);
            let _ = CloseHandle(handle);

            // STILL_ACTIVE = 259
            result != 0 && exit_code == 259
        }
    }

    #[cfg(not(windows))]
    pub fn is_process_alive(process_id: u32) -> bool {
        use std::process::Command;
        let output = Command::new("kill")
            .args(["-0", &process_id.to_string()])
            .output();

        match output {
            Ok(o) => o.status.success(),
            Err(_) => false,
        }
    }
}