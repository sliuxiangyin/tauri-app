use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Plans::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Plans::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Plans::Mid).string().not_null())
                    .col(ColumnDef::new(Plans::NeedAgent).string().default("false"))
                    .col(ColumnDef::new(Plans::Reasoning).text())
                    .col(ColumnDef::new(Plans::Steps).text())
                    .col(ColumnDef::new(Plans::StepResults).text())
                    .col(ColumnDef::new(Plans::StopReason).string())
                    .col(ColumnDef::new(Plans::CompletedAt).integer())
                    .col(
                        ColumnDef::new(Plans::CreatedAt)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_plan_mid")
                    .table(Plans::Table)
                    .col(Plans::Mid)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Plans::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Plans {
    Table,
    Id,
    Mid,
    NeedAgent,
    Reasoning,
    Steps,
    StepResults,
    StopReason,
    CompletedAt,
    CreatedAt,
}