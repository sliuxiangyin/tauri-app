# 数据库迁移文档

本模块使用 [SeaORM Migration](https://www.sea-ql.org/SeaORM/docs/migration/) 管理所有数据库表结构的版本化演进。

## 表关系总览

```text
┌───────────────────────────────┐
│   model_provider_config       │  AI 模型提供商配置
│   └── model_provider_model    │  提供商下的可用模型（FK → config.id，级联删除）
└───────────────────────────────┘

┌───────────────────────────────┐
│   mcp                         │  MCP 服务配置与状态（独立表）
└───────────────────────────────┘

┌───────────────────────────────────────────────────────────────────┐
│   messages（消息索引）                                             │
│   ├── conversations（统一内容块，mid → messages.id，1:N）          │
│   │     block_type: text / thinking / tool_call / tool_result     │
│   │     source: chat（普通对话）/ plan（Plan 执行）                │
│   │     source_id + step_index → plans.id（Plan 关联）            │
│   └── plans（执行计划，mid → messages.id，1:1 可选）               │
│         order_num 与 conversations.order_num 共享排序空间          │
└───────────────────────────────────────────────────────────────────┘
```

### 关系说明

| 关系 | 类型 | 说明 |
|------|------|------|
| `messages` → `conversations` | 1:N | 一条消息包含多个内容块，通过 `conversations.mid` 关联 |
| `messages` → `plans` | 1:1（可选） | 一条 assistant 消息可关联一个执行计划，通过 `plans.mid` 关联 |
| `plans` → `conversations` | 1:N | Plan 执行产生的内容块通过 `conversations.source_id` + `step_index` 关联 |
| `model_provider_config` → `model_provider_model` | 1:N | 一个提供商配置多个模型，通过 `model_provider_model.config_id` 关联（FK 级联删除） |
| `mcp` | 独立表 | 与其他表无关联 |

## 迁移清单

| 序号 | 迁移文件 | 说明 |
|------|----------|------|
| 1 | `m20250512_000001_placeholder` | 占位迁移（已应用，无表操作） |
| 2 | `m20250513_000001_model_provider` | 创建 `model_provider_config` + `model_provider_model` 表 |
| 3 | `m20250514_000001_mcp` | 创建 `mcp` 表（MCP 服务管理） |
| 4 | `m20250515_000001_conversations` | 创建 `conversations` 表（统一内容块模型） |
| 5 | `m20250515_000002_plans` | 创建 `plans` 表（执行计划） |
| 6 | `m20250515_000003_messages` | 创建 `messages` 表（消息索引） |
| 7 | `m20250607_000001_plans_order_num` | 为 `plans` 表新增 `order_num` 列 |

---

## 表结构详情

### model_provider_config — AI 模型提供商配置

> 迁移：`m20250513_000001_model_provider`

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | PK | 主键（UUID） |
| `display_name` | VARCHAR | 显示名称 |
| `enabled` | INTEGER | 是否启用：1=启用 / 0=禁用 |
| `provider_kind` | VARCHAR | 提供商类型 |
| `api_base_url` | VARCHAR | API 基础地址 |
| `api_key` | VARCHAR? | API 密钥（可选） |
| `extra_json` | VARCHAR? | 额外配置 JSON |
| `sort_index` | INTEGER | 排序权重 |
| `created_at` | INTEGER | 创建时间 |
| `updated_at` | INTEGER | 更新时间 |

**索引：** `idx_mpc_enabled_sort`（enabled, sort_index）

### model_provider_model — 提供商可用模型

> 迁移：`m20250513_000001_model_provider`

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | PK | 主键（UUID） |
| `config_id` | FK | → model_provider_config.id（级联删除） |
| `model_id` | VARCHAR | 模型标识符 |
| `model_name` | VARCHAR | 模型显示名称 |
| `group_name` | VARCHAR | 分组名称（默认空串） |
| `sort_index` | INTEGER | 排序权重 |

**索引：** `idx_mpm_config_sort`（config_id, sort_index）

---

### mcp — MCP 服务管理

> 迁移：`m20250514_000001_mcp`

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | PK | 主键（自增整数） |
| `name` | VARCHAR | 服务名称 |
| `transport` | VARCHAR | 传输方式（默认 stdio） |
| `config` | TEXT | 配置 JSON |
| `status` | VARCHAR | 状态（默认 disable） |
| `operating` | VARCHAR | 运行状态（默认 idle） |
| `tools` | TEXT | 工具列表 JSON（默认 []） |
| `error_msg` | TEXT | 错误信息（默认空串） |
| `updated_at` | INTEGER | 更新时间 |

---

### conversations — 统一内容块

> 迁移：`m20250515_000001_conversations`

一条 assistant 消息的实际执行流程可能是：

```text
[Message: assistant]
  ├─ 1. "我来帮你查一下..."           (text)
  ├─ 2. thinking: "需要先搜索..."      (thinking)
  ├─ 3. 调用 search 工具              (tool_call)
  ├─ 4. 工具返回结果                  (tool_result)
  ├─ 5. "根据查询结果..."             (text)
  └── [Plan: 执行计划摘要]            (可选，1:1)
```

每个内容块通过 `block_type` 区分类型，`order_num` 保证块在同一消息内的顺序。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | PK | 主键（UUID v7） |
| `mid` | FK | → messages.id |
| `block_type` | VARCHAR(32) | 内容块类型：text / thinking / tool_call / tool_result |
| `order_num` | INTEGER | 块序号（同一消息内按顺序递增） |
| `source` | VARCHAR(16) | 来源：chat（默认）/ plan |
| `source_id` | VARCHAR(64)? | 关联的 plan_id（Plan 执行时） |
| `step_index` | INTEGER? | Plan 步骤序号（Plan 执行时） |
| `content` | TEXT? | 文本内容（text / tool_result 类型使用） |
| `content_summary` | TEXT? | 内容摘要 |
| `thinking` | TEXT? | 思考过程（thinking 类型使用） |
| `tool_name` | VARCHAR(128)? | 工具名称（tool_call 类型使用） |
| `tool_arguments` | TEXT? | 工具调用参数 JSON（tool_call 类型使用） |
| `tool_output` | TEXT? | 工具执行结果（tool_result 类型使用） |
| `tool_status` | VARCHAR(32)? | 工具执行状态：pending / success / failed |
| `tool_duration_ms` | BIGINT? | 工具执行耗时（毫秒） |
| `tool_error` | TEXT? | 工具错误信息 |
| `extends` | TEXT | 扩展字段 JSON（默认 {}） |
| `attachments` | TEXT? | 附件 JSON 数组 |
| `metadata` | TEXT | 元数据 JSON（默认 {}） |
| `created_at` | BIGINT | 创建时间（Unix 时间戳毫秒数） |

**索引：**
- `idx_conv_mid`（mid）
- `idx_conv_mid_order`（mid, order_num）
- `idx_conv_source`（source, source_id）

---

### plans — 执行计划

> 迁移：`m20250515_000002_plans`，增量迁移：`m20250607_000001_plans_order_num`

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | PK | 主键（UUID） |
| `mid` | FK | → messages.id |
| `order_num` | INTEGER | 块序号，与 conversations.order_num 共享排序空间（增量迁移添加） |
| `need_agent` | VARCHAR | 是否需要 Agent（默认 false） |
| `reasoning` | TEXT? | 推理过程 |
| `steps` | TEXT? | 步骤列表 JSON |
| `step_results` | TEXT? | 步骤执行结果 JSON |
| `stop_reason` | VARCHAR? | 停止原因 |
| `completed_at` | INTEGER? | 完成时间 |
| `created_at` | INTEGER | 创建时间 |

**索引：** `idx_plan_mid`（mid）

---

### messages — 消息索引

> 迁移：`m20250515_000003_messages`

`messages` 表只存储消息的**索引/元数据**，不包含内容正文。内容存储在 `conversations` 表（统一内容块），通过 `mid` 关联。

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | PK | 主键（UUID v7） |
| `account_id` | VARCHAR | 账号 ID（多账号隔离） |
| `chat_type` | VARCHAR | 聊天类型：client / wechat |
| `session_id` | VARCHAR | 会话 ID |
| `parent_id` | VARCHAR? | 父消息 ID（消息树结构） |
| `role` | VARCHAR | 消息角色：user / assistant / system / tool |
| `status` | VARCHAR | 消息状态：pending / completed / failed |
| `token_usage` | TEXT? | Token 使用量 JSON |
| `created_at` | BIGINT | 创建时间（Unix 时间戳毫秒数） |
| `is_deleted` | VARCHAR | 软删除标记：0=正常 / 1=已删除 |

**索引：**
- `idx_msg_account_session`（account_id, chat_type, session_id, created_at）
- `idx_msg_parent`（parent_id）

---

## 设计决策

### conversations.order_num 与 plans.order_num 共享排序空间

`plans.order_num`（增量迁移 `m20250607` 添加）与 `conversations.order_num` 共享排序空间，保证 plan 和 content blocks 在 DTO 层可以按顺序合并展示。
