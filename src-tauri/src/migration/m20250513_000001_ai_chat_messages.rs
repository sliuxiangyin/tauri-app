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
                            .integer()
                            .not_null()
                            .default(0),
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
