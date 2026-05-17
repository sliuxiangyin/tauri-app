use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;
/**
 * CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,              -- 微信账号 ID（多账号隔离）
    chat_type TEXT NOT NULL DEFAULT 'ai', -- 会话类型：ai / wechat / group
    session_id TEXT NOT NULL,
    parent_message_id TEXT,

    role TEXT NOT NULL,                -- user / assistant / system / tool
    content TEXT,                      -- 完整可见文本
    content_summary TEXT,              -- 超长摘要
    thinking TEXT,                     -- 思考链文本

    -- 工具调用
    tool_calls TEXT,                   -- assistant 发起调用的 JSON
    tool_call_id TEXT,                 -- tool 消息关联的调用 ID
    tool_output TEXT,                  -- 工具返回原始数据

    -- 扩展字段
    extends TEXT DEFAULT '{}',         -- JSON，存放 skill_id, agent_id, ... 等

    -- 附件
    attachments TEXT,                  -- JSON 数组

    status TEXT DEFAULT 'completed',
    token_usage TEXT,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    metadata TEXT DEFAULT '{}',

    -- 软删除
    is_deleted TEXT DEFAULT '0'
);

-- 索引
CREATE INDEX idx_msg_account_session ON messages(account_id, chat_type, session_id, created_at);
CREATE INDEX idx_msg_parent ON messages(parent_message_id);
CREATE INDEX idx_msg_tool_call ON messages(tool_call_id);
 */
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

        manager
            .create_index(
                Index::create()
                    .name("idx_msg_account_session")
                    .table(Messages::Table)
                    .col(Messages::AccountId)
                    .col(Messages::ChatType)
                    .col(Messages::SessionId)
                    .col(Messages::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_msg_parent")
                    .table(Messages::Table)
                    .col(Messages::ParentMessageId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_msg_tool_call")
                    .table(Messages::Table)
                    .col(Messages::ToolCallId)
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
