//! # Conversations 表 - 统一内容块模型
//!
//! ## 设计思路
//!
//! 一条 assistant 消息的实际执行流程可能是：
//! ```text
//! [Message: assistant]
//!   ├─ 1. "我来帮你查一下..."           (text)
//!   ├─ 2. thinking: "需要先搜索..."      (thinking)
//!   ├─ 3. 调用 search 工具              (tool_call)
//!   ├─ 4. 工具返回结果                  (tool_result)
//!   ├─ 5. "根据查询结果..."             (text)
//!   └─ [Plan: 执行计划摘要]             (可选，1:1)
//! ```
//!
//! 将 `conversations` 表升级为**统一内容块**概念：
//! - 每个内容块通过 `block_type` 区分类型：`text` / `thinking` / `tool_call` / `tool_result`
//! - `order_num` 保证块在同一消息内的顺序
//! - `source` 区分来源：`chat`（普通对话）/ `plan`（Plan 执行）
//! - Plan 执行产生的内容块通过 `source_id` 关联到 `plans.id`，`step_index` 标识第几步
//!
//! ## 整体数据关系
//!
//! ```text
//! messages（消息索引）
//! ├── conversations（内容块，按 order_num 排序）
//! │     block_type: text / thinking / tool_call / tool_result
//! │     source: chat / plan
//! └── plans（执行计划，可选，1:1）
//! ```
//!
//! ## 为什么废弃 tool_calls 表
//!
//! 旧的 `tool_calls` 表通过 `type` + `type_id` 交叉引用 conversations 或 plans，
//! 无法自然表达 "对话 → 调用工具 → 继续对话" 的时间线。
//! 将工具调用信息内嵌到 conversations 内容块后：
//! - 结构更清晰：message → blocks + plan，无交叉引用
//! - 级联删除更简单：删 message 时直接删 `WHERE mid = ?`
//! - 前端渲染更友好：一个消息就是一个完整的渲染单元
//!
//! ## 字段说明
//!
//! | 字段 | 类型 | 说明 |
//! |---|---|---|
//! | `id` | PK | 主键（UUID v7） |
//! | `mid` | FK | → messages.id |
//! | `block_type` | VARCHAR | 内容块类型：text / thinking / tool_call / tool_result |
//! | `order_num` | INTEGER | 块序号（同一消息内按顺序递增） |
//! | `source` | VARCHAR | 来源：chat（默认）/ plan |
//! | `source_id` | VARCHAR? | 关联的 plan_id（Plan 执行时） |
//! | `step_index` | INTEGER? | Plan 步骤序号（Plan 执行时） |
//! | `content` | TEXT? | 文本内容（text / tool_result 类型使用） |
//! | `content_summary` | TEXT? | 内容摘要 |
//! | `thinking` | TEXT? | 思考过程（thinking 类型使用） |
//! | `tool_name` | VARCHAR? | 工具名称（tool_call 类型使用） |
//! | `tool_arguments` | TEXT? | 工具调用参数 JSON（tool_call 类型使用） |
//! | `tool_output` | TEXT? | 工具执行结果（tool_result 类型使用） |
//! | `tool_status` | VARCHAR? | 工具执行状态：pending / success / failed |
//! | `tool_duration_ms` | BIGINT? | 工具执行耗时（毫秒） |
//! | `tool_error` | TEXT? | 工具错误信息 |
//! | `extends` | TEXT | 扩展字段 JSON |
//! | `attachments` | TEXT? | 附件 JSON 数组 |
//! | `metadata` | TEXT | 元数据 JSON |
//! | `created_at` | BIGINT | 创建时间（Unix 时间戳毫秒数） |

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Conversations::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Conversations::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Conversations::Mid).string().not_null())
                    // ── 内容块核心字段 ──
                    .col(
                        ColumnDef::new(Conversations::BlockType)
                            .string_len(32)
                            .not_null()
                            .default("text"),
                    )
                    .col(
                        ColumnDef::new(Conversations::OrderNum)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Conversations::Source)
                            .string_len(16)
                            .not_null()
                            .default("chat"),
                    )
                    .col(ColumnDef::new(Conversations::SourceId).string_len(64))
                    .col(ColumnDef::new(Conversations::StepIndex).integer())
                    // ── 文本内容字段 ──
                    .col(ColumnDef::new(Conversations::Content).text())
                    .col(ColumnDef::new(Conversations::ContentSummary).text())
                    .col(ColumnDef::new(Conversations::Thinking).text())
                    // ── 工具调用字段 ──
                    .col(ColumnDef::new(Conversations::ToolName).string_len(128))
                    .col(ColumnDef::new(Conversations::ToolArguments).text())
                    .col(ColumnDef::new(Conversations::ToolOutput).text())
                    .col(ColumnDef::new(Conversations::ToolStatus).string_len(32))
                    .col(ColumnDef::new(Conversations::ToolDurationMs).big_integer())
                    .col(ColumnDef::new(Conversations::ToolError).text())
                    // ── 通用字段 ──
                    .col(ColumnDef::new(Conversations::Extends).text().default("{}"))
                    .col(ColumnDef::new(Conversations::Attachments).text())
                    .col(ColumnDef::new(Conversations::Metadata).text().default("{}"))
                    .col(
                        ColumnDef::new(Conversations::CreatedAt)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        // 索引：按 mid 查询内容块（兼容旧查询）
        manager
            .create_index(
                Index::create()
                    .name("idx_conv_mid")
                    .table(Conversations::Table)
                    .col(Conversations::Mid)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // 索引：按 mid + order_num 查询内容块（保证顺序）
        manager
            .create_index(
                Index::create()
                    .name("idx_conv_mid_order")
                    .table(Conversations::Table)
                    .col(Conversations::Mid)
                    .col(Conversations::OrderNum)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // 索引：按 source + source_id 查询 Plan 相关内容块
        manager
            .create_index(
                Index::create()
                    .name("idx_conv_source")
                    .table(Conversations::Table)
                    .col(Conversations::Source)
                    .col(Conversations::SourceId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Conversations::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Conversations {
    Table,
    Id,
    Mid,
    // 内容块核心字段
    BlockType,
    OrderNum,
    Source,
    SourceId,
    StepIndex,
    // 文本内容字段
    Content,
    ContentSummary,
    Thinking,
    // 工具调用字段
    ToolName,
    ToolArguments,
    ToolOutput,
    ToolStatus,
    ToolDurationMs,
    ToolError,
    // 通用字段
    Extends,
    Attachments,
    Metadata,
    CreatedAt,
}
