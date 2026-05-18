use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(McpServeConfig::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(McpServeConfig::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(McpServeConfig::Name).string().not_null())
                    .col(ColumnDef::new(McpServeConfig::Config).text().not_null())
                    .col(
                        ColumnDef::new(McpServeConfig::UpdatedAt)
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
            .drop_table(Table::drop().table(McpServeConfig::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum McpServeConfig {
    Table,
    Id,
    Name,
    Config,
    UpdatedAt,
}
