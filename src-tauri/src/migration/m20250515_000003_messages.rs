//! # Messages 表 - 消息索引
//!
//! ## 设计思路
//!
//! `messages` 表只存储消息的**索引/元数据**，不包含内容正文。
//! 内容存储在 `conversations` 表（统一内容块），通过 `mid` 关联。
//!
//! ## 数据关系
//!
//! ```text
//! messages（消息索引）          ← 本表
//! ├── conversations（内容块）   通过 mid 关联，1:N
//! └── plans（执行计划）         通过 mid 关联，1:1（可选）
//! ```
//!
//! ## 与旧 chat_messages 表的区别
//!
//! | | 旧表（chat_messages） | 新表（messages） |
//! |---|---|---|
//! | 内容存储 | 内嵌 content/thinking/tool_calls | 拆分到 conversations 表 |
//! | 工具调用 | tool_calls/tool_call_id 字段 | 通过 conversations block_type 表达 |
//! | 扩展性 | 扁平结构，难以扩展 | 内容块模型，灵活扩展 |
//!
//! ## 字段说明
//!
//! | 字段 | 类型 | 说明 |
//! |---|---|---|
//! | `id` | PK | 主键（UUID v7） |
//! | `account_id` | VARCHAR | 账号 ID（多账号隔离） |
//! | `chat_type` | VARCHAR | 聊天类型：client / wechat |
//! | `session_id` | VARCHAR | 会话 ID |
//! | `parent_id` | VARCHAR? | 父消息 ID（消息树结构） |
//! | `role` | VARCHAR | 消息角色：user / assistant / system / tool |
//! | `status` | VARCHAR | 消息状态：pending / completed / failed |
//! | `token_usage` | TEXT? | Token 使用量 JSON |
//! | `created_at` | BIGINT | 创建时间（Unix 时间戳毫秒数） |
//! | `is_deleted` | VARCHAR | 软删除标记：0=正常 / 1=已删除 |

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Messages::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Messages::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Messages::AccountId).string().not_null())
                    .col(
                        ColumnDef::new(Messages::ChatType)
                            .string()
                            .not_null()
                            .default("client"),
                    )
                    .col(ColumnDef::new(Messages::SessionId).string().not_null())
                    .col(ColumnDef::new(Messages::ParentId).string())
                    .col(ColumnDef::new(Messages::Role).string().not_null())
                    .col(
                        ColumnDef::new(Messages::Status)
                            .string()
                            .default("completed"),
                    )
                    .col(ColumnDef::new(Messages::TokenUsage).text())
                    .col(
                        ColumnDef::new(Messages::CreatedAt)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Messages::IsDeleted)
                            .string()
                            .default("0"),
                    )
                    .to_owned(),
            )
            .await?;

        // 复合索引：按账号+类型+会话+时间查询消息列表
        manager
            .create_index(
                Index::create()
                    .name("idx_msg_account_session")
                    .table(Messages::Table)
                    .col(Messages::AccountId)
                    .col(Messages::ChatType)
                    .col(Messages::SessionId)
                    .col(Messages::CreatedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // 索引：按父消息 ID 查询子消息
        manager
            .create_index(
                Index::create()
                    .name("idx_msg_parent")
                    .table(Messages::Table)
                    .col(Messages::ParentId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Messages::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Messages {
    Table,
    Id,
    AccountId,
    ChatType,
    SessionId,
    ParentId,
    Role,
    Status,
    TokenUsage,
    CreatedAt,
    IsDeleted,
}
