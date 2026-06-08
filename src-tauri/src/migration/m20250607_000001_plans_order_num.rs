//! 给 plans 表新增 order_num 列
//!
//! 用于与 conversations.order_num 共享排序空间，
//! 保证 plan 和 content blocks 在 DTO 层可以按顺序合并展示。
//!
//! | 字段         | 类型    | 说明                          |
//! |--------------|---------|-------------------------------|
//! | `order_num`  | INTEGER | 块序号（默认 0）              |

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Plans::Table)
                    .add_column(
                        ColumnDef::new(Plans::OrderNum)
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
            .alter_table(
                Table::alter()
                    .table(Plans::Table)
                    .drop_column(Plans::OrderNum)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Plans {
    Table,
    OrderNum,
}
