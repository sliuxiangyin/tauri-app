use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Mcp::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Mcp::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Mcp::Name).string().not_null())
                    .col(ColumnDef::new(Mcp::Transport).string().not_null().default("stdio"))
                    .col(ColumnDef::new(Mcp::Config).text().not_null())
                    .col(ColumnDef::new(Mcp::Status).string().default("disable"))
                    .col(ColumnDef::new(Mcp::Operating).string().default("idle"))
                    .col(ColumnDef::new(Mcp::Tools).text().default("[]"))
                    .col(ColumnDef::new(Mcp::ErrorMsg).text().default(""))
                    .col(
                        ColumnDef::new(Mcp::UpdatedAt)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Mcp::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Mcp {
    Table,
    Id,
    Name,
    Transport,
    Config,
    Status,
    Operating,
    Tools,
    ErrorMsg,
    UpdatedAt,
}