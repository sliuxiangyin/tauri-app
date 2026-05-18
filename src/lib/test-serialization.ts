// 测试 Tauri 序列化/反序列化行为
// 用法：在控制台或代码中调用 testTauriSerialization()

/**
 * 测试 Rust struct 是否自动转换为 camelCase
 * 
 * 测试步骤：
 * 1. 调用 wechat_get_accounts 获取后端数据
 * 2. 检查返回对象的字段名是 account_id 还是 accountId
 */

export async function testTauriSerialization() {
  const { invoke } = await import("@tauri-apps/api/core");

  console.log("=== Tauri 序列化测试开始 ===");

  // 测试 1: 调用 wechat_get_accounts
  console.log("\n[Test 1] 调用2 wechat_get_accounts...");
  const response = await invoke("wechat_get_accounts");
  console.log("原始响应1:", response);
   
  
  // 测试 2: 直接 invoke 返回并检查属性
  console.log("\n=== 测试完成 ===");
}

