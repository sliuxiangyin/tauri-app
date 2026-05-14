use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // --- model_provider_config ---
        manager
            .create_table(
                Table::create()
                    .table(ModelProviderConfig::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ModelProviderConfig::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ModelProviderConfig::DisplayName)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ModelProviderConfig::Enabled)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(ModelProviderConfig::ProviderKind)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ModelProviderConfig::ApiBaseUrl)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(ModelProviderConfig::ApiKey).string().null())
                    .col(
                        ColumnDef::new(ModelProviderConfig::ExtraJson)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ModelProviderConfig::SortIndex)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(ModelProviderConfig::CreatedAt)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ModelProviderConfig::UpdatedAt)
                            .string()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_mpc_enabled_sort")
                    .table(ModelProviderConfig::Table)
                    .col(ModelProviderConfig::Enabled)
                    .col(ModelProviderConfig::SortIndex)
                    .to_owned(),
            )
            .await?;

        // --- model_provider_model ---
        manager
            .create_table(
                Table::create()
                    .table(ModelProviderModel::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ModelProviderModel::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ModelProviderModel::ConfigId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ModelProviderModel::ModelId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ModelProviderModel::ModelName)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ModelProviderModel::GroupName)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(ModelProviderModel::SortIndex)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_mpm_config_id")
                            .from(ModelProviderModel::Table, ModelProviderModel::ConfigId)
                            .to(ModelProviderConfig::Table, ModelProviderConfig::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_mpm_config_sort")
                    .table(ModelProviderModel::Table)
                    .col(ModelProviderModel::ConfigId)
                    .col(ModelProviderModel::SortIndex)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ModelProviderModel::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ModelProviderConfig::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum ModelProviderConfig {
    Table,
    Id,
    DisplayName,
    Enabled,
    ProviderKind,
    ApiBaseUrl,
    ApiKey,
    ExtraJson,
    SortIndex,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum ModelProviderModel {
    Table,
    Id,
    ConfigId,
    ModelId,
    ModelName,
    GroupName,
    SortIndex,
}
