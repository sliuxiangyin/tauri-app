# OpenClaw Weixin API 文档

## 基础信息
- **服务地址**: `http://localhost:3000` (默认端口)
- **状态目录**: `./state/` (默认)

## 命令行参数

可执行文件支持以下参数：

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `--port` | HTTP 服务端口 | `3000` |
| `--state-path` | 状态/缓存目录路径 | `./state` |
| `--webhook` | Webhook 回调地址（预留） | 无 |

### 使用示例

```bash
# 使用默认配置
./openclaw-weixin

# 自定义端口和状态目录
./openclaw-weixin --port 8080 --state-path /data/state

# 完整配置
./openclaw-weixin --port 8080 --state-path /data/state --webhook https://example.com/webhook
```

---

## 1. SSE 登录流 ⭐ 推荐

**接口**: `GET /login/stream`

**用途**: 实时推送微信登录状态（二维码生成、扫码、过期重试、登录成功等）

**请求**:
```bash
curl -N http://localhost:3000/login/stream?accountId=test-account
```

**SSE 事件流**:

| 事件类型 | 说明 | 数据字段 | 触发时机 |
|---------|------|---------|----------|
| `qr_generated` | 二维码已生成 | `qrDataUrl`, `sessionKey`, `message` | 登录开始，二维码生成完成 |
| `scanned` | 用户已扫码 | `message` | 用户使用微信扫描二维码 |
| `qr_expired` | 二维码过期 | `retryCount`, `maxRetries`, `message` | 二维码过期，自动刷新（最多 3 次） |
| `confirmed` | 登录已确认 | `accountId`, `message` | 用户在微信中确认登录 |
| `login_success` | 登录成功 | `accountId`, `message` | 成功建立微信连接 |
| `login_failed` | 登录失败 | `message` | 登录过程中发生错误 |
| `error` | 通用错误 | `message` | 其他异常情况 |

**响应示例**:
```
event: qr_generated
data: {"qrDataUrl":"https://liteapp.weixin.qq.com/q/7GiQu1?qrcode=bf7a38f2b11428ec3a4b74985da9a241&bot_type=3","sessionKey":"default","message":"请使用微信扫描二维码"}

event: scanned
data: {"message":"已扫码，请在微信中确认"}

event: qr_expired
data: {"message":"二维码已过期，正在刷新","retryCount":1,"maxRetries":3}

event: confirmed
data: {"accountId":"bf0f82d178a9@im.bot","message":"登录已确认"}

event: login_success
data: {"accountId":"bf0f82d178a9@im.bot","message":"✅ 与微信连接成功！"}
```

**特点**:
- ✅ 实时推送所有状态变化
- ✅ 支持二维码过期自动重试（最多 3 次）
- ✅ 客户端无需轮询
- ✅ 支持多个客户端同时监听

---

## 2. 发送消息

**接口**: `POST /message/send`

**用途**: 发送文本消息

**请求**:
```bash
curl -X POST http://localhost:3000/message/send \
  -H "Content-Type: application/json" \
  -d '{
    "accountId": "test-account",
    "to": "user_openid",
    "text": "Hello, World!"
  }'
```

**响应示例**:
```json
{
  "success": true,
  "messageId": "msg_123456",
  "channel": "openclaw-weixin"
}
```

---

## 3. 获取账号列表

**接口**: `GET /accounts`

**用途**: 获取所有账号及其运行状态

**请求**:
```bash
curl -X GET http://localhost:3000/accounts
```

**响应示例**:
```json
{
  "success": true,
  "accountIds": ["test-account"],
  "accounts": [
    {
      "accountId": "test-account",
      "configured": true,
      "enabled": true,
      "running": true
    }
  ],
  "count": 1,
  "runningCount": 1
}
```

---

## 5. Webhook 回调

**用途**: 接收微信消息的实时推送（当收到用户消息时）

**配置**:
启动时通过 `--webhook` 参数指定回调地址：
```bash
./openclaw-weixin --webhook https://example.com/webhook
```

**触发时机**: 当收到用户发送的消息时（包括文本、图片、语音、文件等）

**请求方法**: `POST`

**Content-Type**: `application/json`

### Webhook 接收数据类型

Webhook 会推送以下字段的消息对象：

| 字段名 | 类型 | 说明 | 示例 |
|--------|------|------|------|
| `Body` | string | 消息体内容（文本消息的内容，或媒体消息的摘要） | `"Hello"` |
| `From` | string | 消息发送者 ID（微信用户 OpenID） | `"o9cq80xP5A5ajveONBzpVNuDmdcA@im.wechat"` |
| `To` | string | 消息接收者 ID（通常与 From 相同，表示私聊） | `"o9cq80xP5A5ajveONBzpVNuDmdcA@im.wechat"` |
| `AccountId` | string | 账号 ID（机器人账号标识） | `"bf0f82d178a9-im-bot"` |
| `OriginatingChannel` | string | 原始渠道标识，固定为 `"openclaw-weixin"` | `"openclaw-weixin"` |
| `OriginatingTo` | string | 原始接收者 | `"o9cq80xP5A5ajveONBzpVNuDmdcA@im.wechat"` |
| `MessageSid` | string | 消息唯一标识 | `"openclaw-weixin:1778737726819-b7173118"` |
| `Timestamp` | number | 消息时间戳（毫秒） | `1778737725160` |
| `Provider` | string | 提供者标识，固定为 `"openclaw-weixin"` | `"openclaw-weixin"` |
| `ChatType` | string | 聊天类型，当前仅支持 `"direct"`（私聊） | `"direct"` |
| `context_token` | string | 上下文令牌，用于验证和跟踪会话 | `"AARzJWAFAAAB..."` |
| `MediaPath` | string? | 媒体文件本地路径（仅媒体消息） | `"state/media/inbound/media_1778737726817_2iobs9.bin"` |
| `MediaType` | string? | 媒体文件 MIME 类型（仅媒体消息） | `"image/*"`, `"video/mp4"`, `"audio/wav"`, `"application/pdf"` |
| `CommandBody` | string | 命令体内容（用户发送的命令参数） | `""` |
| `CommandAuthorized` | boolean | 命令授权状态 | `true` |

### Webhook 推送示例

**文本消息**:
```json
{
  "Body": "你好",
  "From": "o9cq80xP5A5ajveONBzpVNuDmdcA@im.wechat",
  "To": "o9cq80xP5A5ajveONBzpVNuDmdcA@im.wechat",
  "AccountId": "bf0f82d178a9-im-bot",
  "OriginatingChannel": "openclaw-weixin",
  "OriginatingTo": "o9cq80xP5A5ajveONBzpVNuDmdcA@im.wechat",
  "MessageSid": "openclaw-weixin:1778737726819-b7173118",
  "Timestamp": 1778737725160,
  "Provider": "openclaw-weixin",
  "ChatType": "direct",
  "context_token": "AARzJWAFAAAB...",
  "CommandBody": "",
  "CommandAuthorized": true
}
```

**图片消息**:
```json
{
  "Body": "",
  "From": "o9cq80xP5A5ajveONBzpVNuDmdcA@im.wechat",
  "To": "o9cq80xP5A5ajveONBzpVNuDmdcA@im.wechat",
  "AccountId": "bf0f82d178a9-im-bot",
  "OriginatingChannel": "openclaw-weixin",
  "OriginatingTo": "o9cq80xP5A5ajveONBzpVNuDmdcA@im.wechat",
  "MessageSid": "openclaw-weixin:1778737726819-b7173118",
  "Timestamp": 1778737725160,
  "Provider": "openclaw-weixin",
  "ChatType": "direct",
  "context_token": "AARzJWAFAAAB...",
  "MediaPath": "state/media/inbound/media_1778737726817_2iobs9.bin",
  "MediaType": "image/*",
  "CommandBody": "",
  "CommandAuthorized": true
}
```

### Webhook 处理示例

**Node.js 示例**:
```javascript
const express = require('express');
const app = express();

app.use(express.json());

app.post('/webhook', (req, res) => {
  const message = req.body;
  
  console.log(`收到消息 from ${message.From}:`);
  console.log(`消息内容：${message.Body}`);
  
  if (message.MediaPath) {
    console.log(`媒体文件：${message.MediaPath} (${message.MediaType})`);
  }
  
  // 处理消息逻辑...
  
  res.status(200).send('OK');
});

app.listen(8080, () => {
  console.log('Webhook server listening on port 8080');
});
```

**Python 示例**:
```python
from flask import Flask, request, jsonify

app = Flask(__name__)

@app.route('/webhook', methods=['POST'])
def webhook():
    message = request.json
    
    print(f"收到消息 from {message['From']}:")
    print(f"消息内容：{message['Body']}")
    
    if message.get('MediaPath'):
        print(f"媒体文件：{message['MediaPath']} ({message['MediaType']})")
    
    # 处理消息逻辑...
    
    return jsonify({'status': 'ok'}), 200

if __name__ == '__main__':
    app.run(port=8080)
```

### 注意事项

1. **Webhook 是可选的**: 如果不配置 `--webhook` 参数，消息不会推送
2. **推送时机**: 每条用户消息都会触发一次 Webhook 推送
3. **媒体文件**: 媒体消息会先下载到本地，然后推送文件路径
4. **错误处理**: 如果 Webhook 返回非 2xx 状态码，会记录错误日志但不会重试
5. **会话跟踪**: `context_token` 可用于跟踪同一会话的消息

---

## 完整测试流程

### 步骤 1: 启动服务器
```bash
# 在项目目录下
node main.ts
```

### 步骤 2: 使用 SSE 登录（推荐）

打开一个终端窗口，运行：
```bash
curl -N http://localhost:3000/login/stream?accountId=test-account
```

你会看到实时推送的登录状态，完整事件流程如下：

**正常登录流程**：
1. `qr_generated` - 二维码已生成（包含二维码 URL）
2. `scanned` - 用户已扫码
3. `confirmed` - 用户确认登录
4. `login_success` - 登录成功，建立连接

**异常情况**：
- `qr_expired` - 二维码过期（自动刷新，最多 3 次）
- `login_failed` - 登录失败（如网络错误、服务器错误等）
- `error` - 其他异常错误

### 步骤 3: 检查账号状态
```bash
curl -X GET http://localhost:3000/accounts
```

### 步骤 4: 发送测试消息
```bash
curl -X POST http://localhost:3000/message/send \
  -H "Content-Type: application/json" \
  -d '{
    "accountId": "test-account",
    "to": "user_openid",
    "text": "测试消息"
  }'
```

---

## JavaScript 客户端示例

```javascript
// 使用 EventSource 监听登录流
const accountId = 'test-account';
const eventSource = new EventSource(`http://localhost:3000/login/stream?accountId=${accountId}`);

// 监听二维码生成
eventSource.addEventListener('qr_generated', (event) => {
  const data = JSON.parse(event.data);
  console.log('📱 二维码已生成:', data.qrDataUrl);
  // 显示二维码
  document.getElementById('qr-image').src = data.qrDataUrl;
});

// 监听已扫码
eventSource.addEventListener('scanned', (event) => {
  const data = JSON.parse(event.data);
  console.log('👀 已扫码:', data.message);
  // 更新 UI 提示
  document.getElementById('status').textContent = '已扫码，请在微信中确认';
});

// 监听登录确认
eventSource.addEventListener('confirmed', (event) => {
  const data = JSON.parse(event.data);
  console.log('✅ 登录已确认:', data.accountId);
  // 更新 UI 提示
  document.getElementById('status').textContent = '登录已确认，正在建立连接...';
});

// 监听二维码过期
eventSource.addEventListener('qr_expired', (event) => {
  const data = JSON.parse(event.data);
  console.log(`⏳ 二维码过期 (${data.retryCount}/${data.maxRetries})`);
  // 显示刷新提示
  document.getElementById('status').textContent = `二维码过期，正在刷新 (${data.retryCount}/${data.maxRetries})`;
});

// 监听登录成功
eventSource.addEventListener('login_success', (event) => {
  const data = JSON.parse(event.data);
  console.log('🎉 登录成功:', data.accountId);
  eventSource.close();
  // 跳转到下一步或启动账号
  document.getElementById('status').textContent = '登录成功！';
});

// 监听登录失败
eventSource.addEventListener('login_failed', (event) => {
  const data = JSON.parse(event.data);
  console.error('❌ 登录失败:', data.message);
  eventSource.close();
  // 显示错误提示
  document.getElementById('status').textContent = '登录失败：' + data.message;
});

// 监听错误
eventSource.addEventListener('error', (event) => {
  const data = JSON.parse(event.data);
  console.error('💥 发生错误:', data.message);
  eventSource.close();
  // 显示错误提示
  document.getElementById('status').textContent = '错误：' + data.message;
});

// 8 分钟后自动关闭（登录超时）
setTimeout(() => eventSource.close(), 480000);
```

---

## 错误处理

### 常见错误响应

**未找到账号**:
```json
{
  "success": false,
  "error": "No accounts found. Please login first.",
  "count": 0
}
```

**服务器错误**:
```json
{
  "error": "错误信息",
  "stack": "堆栈跟踪"
}
```

---

## 使用 jq 美化输出

如果系统安装了 `jq`，可以在命令后添加 `| jq` 来美化 JSON 输出：

```bash
# 获取账号列表并美化输出
curl -s http://localhost:3000/accounts | jq

# 发送消息并美化输出
curl -X POST http://localhost:3000/message/send \
  -H "Content-Type: application/json" \
  -d '{"accountId":"test-account","to":"user_openid","text":"Hello"}' | jq
```

---

## 注意事项

1. **登录流程**: 必须先通过 `/login/stream` 完成登录
2. **状态目录**: 所有配置存储在 `./state/` 目录下
3. **二维码过期**: 登录时二维码会自动重试最多 3 次，通过 SSE 实时通知
4. **客户端断开**: 如果客户端断开 SSE 连接，登录流程会自动取消
5. **事件顺序**: 正常登录流程的事件顺序为：`qr_generated` → `scanned` → `confirmed` → `login_success`
6. **二维码刷新**: 如果二维码过期，会自动刷新（最多 3 次），每次刷新会先推送 `qr_expired` 事件，然后推送新的 `qr_generated` 事件
7. **Webhook**: 可选配置，用于接收实时消息推送

---

## SSE vs 传统轮询

| 特性 | SSE (推荐) | 传统轮询 |
|------|-----------|---------|
| 实时性 | ✅ 服务端主动推送 | ❌ 需要定时轮询 |
| 资源消耗 | ✅ 单个长连接 | ❌ 多次 HTTP 请求 |
| 用户体验 | ✅ 立即反馈状态 | ❌ 延迟感知状态 |
| 实现复杂度 | ✅ 简单 | ❌ 需要轮询逻辑 |
| 二维码过期处理 | ✅ 自动推送重试 | ❌ 需要客户端判断 |

---

## 更新日志

- **2026-05-14**: 新增 SSE 登录流，移除传统轮询接口
- **2026-05-14**: 支持二维码过期自动重试（最多 3 次）
- **2026-05-14**: 修复账号重复启动问题
- **2026-05-14**: 修复 HTTP 请求堵塞问题
