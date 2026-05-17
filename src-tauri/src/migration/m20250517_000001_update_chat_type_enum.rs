use sea_orm_migration::prelude::*;

/**
 * 更新 messages 表的 chat_type 枚举值
 * 从 ai/wechat/group 改为 client/wechat
 */
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite 不支持 ALTER COLUMN，需要重建表
        // 0. 先删除可能存在的旧表（之前迁移失败可能留下）
        let _ = manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS messages_old")
            .await;

        // 1. 重命名旧表
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE messages RENAME TO messages_old")
            .await?;

        // 2. 创建新表，使用新的 chat_type CHECK 约束
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
                    .col(ColumnDef::new(Messages::ParentMessageId).string())
                    .col(ColumnDef::new(Messages::Role).string().not_null())
                    .col(ColumnDef::new(Messages::Content).text())
                    .col(ColumnDef::new(Messages::ContentSummary).text())
                    .col(ColumnDef::new(Messages::Thinking).text())
                    .col(ColumnDef::new(Messages::ToolCalls).text())
                    .col(ColumnDef::new(Messages::ToolCallId).string())
                    .col(ColumnDef::new(Messages::ToolOutput).text())
                    .col(ColumnDef::new(Messages::Extends).text().default("{}"))
                    .col(ColumnDef::new(Messages::Attachments).text())
                    .col(
                        ColumnDef::new(Messages::Status)
                            .string()
                            .default("completed"),
                    )
                    .col(ColumnDef::new(Messages::TokenUsage).text())
                    .col(
                        ColumnDef::new(Messages::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(SimpleExpr::Keyword(Keyword::CurrentTimestamp)),
                    )
                    .col(ColumnDef::new(Messages::Metadata).text().default("{}"))
                    .col(ColumnDef::new(Messages::IsDeleted).string().default("0"))
                    .to_owned(),
            )
            .await?;

        // 3. 迁移数据（将 ai 映射为 client）
        manager
            .get_connection()
            .execute_unprepared(
                r#"INSERT INTO messages (id, account_id, chat_type, session_id, parent_message_id, role, content, content_summary, thinking, tool_calls, tool_call_id, tool_output, extends, attachments, status, token_usage, created_at, metadata, is_deleted)
               SELECT id, account_id, 
                      CASE WHEN chat_type = 'ai' THEN 'client' ELSE chat_type END,
                      session_id, parent_message_id, role, content, content_summary, thinking, tool_calls, tool_call_id, tool_output, extends, attachments, status, token_usage, created_at, metadata, is_deleted
               FROM messages_old"#
            )
            .await?;

        // 4. 删除旧表
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE messages_old")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 回滚：恢复原始的 chat_type 枚举值
        // 0. 先删除可能存在的旧表（之前迁移失败可能留下）
        let _ = manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS messages_old")
            .await;

        // 重命名表
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE messages RENAME TO messages_old")
            .await?;

        // 创建新表
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
                            .default("ai"),
                    )
                    .col(ColumnDef::new(Messages::SessionId).string().not_null())
                    .col(ColumnDef::new(Messages::ParentMessageId).string())
                    .col(ColumnDef::new(Messages::Role).string().not_null())
                    .col(ColumnDef::new(Messages::Content).text())
                    .col(ColumnDef::new(Messages::ContentSummary).text())
                    .col(ColumnDef::new(Messages::Thinking).text())
                    .col(ColumnDef::new(Messages::ToolCalls).text())
                    .col(ColumnDef::new(Messages::ToolCallId).string())
                    .col(ColumnDef::new(Messages::ToolOutput).text())
                    .col(ColumnDef::new(Messages::Extends).text().default("{}"))
                    .col(ColumnDef::new(Messages::Attachments).text())
                    .col(
                        ColumnDef::new(Messages::Status)
                            .string()
                            .default("completed"),
                    )
                    .col(ColumnDef::new(Messages::TokenUsage).text())
                    .col(
                        ColumnDef::new(Messages::CreatedAt)
                            .date_time()
                            .not_null()
                            .default(SimpleExpr::Keyword(Keyword::CurrentTimestamp)),
                    )
                    .col(ColumnDef::new(Messages::Metadata).text().default("{}"))
                    .col(ColumnDef::new(Messages::IsDeleted).string().default("0"))
                    .to_owned(),
            )
            .await?;

        // 迁移数据（将 client 映射回 ai）
        manager
            .get_connection()
            .execute_unprepared(
                r#"INSERT INTO messages (id, account_id, chat_type, session_id, parent_message_id, role, content, content_summary, thinking, tool_calls, tool_call_id, tool_output, extends, attachments, status, token_usage, created_at, metadata, is_deleted)
               SELECT id, account_id, 
                      CASE WHEN chat_type = 'client' THEN 'ai' ELSE chat_type END,
                      session_id, parent_message_id, role, content, content_summary, thinking, tool_calls, tool_call_id, tool_output, extends, attachments, status, token_usage, created_at, metadata, is_deleted
               FROM messages_old"#
            )
            .await?;

        // 删除旧表
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE messages_old")
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
    ParentMessageId,
    Role,
    Content,
    ContentSummary,
    Thinking,
    ToolCalls,
    ToolCallId,
    ToolOutput,
    Extends,
    Attachments,
    Status,
    TokenUsage,
    CreatedAt,
    Metadata,
    IsDeleted,
}
