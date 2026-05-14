pub use sea_orm_migration::prelude::*;

mod m20250512_000001_placeholder;
mod m20250513_000001_model_provider;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250512_000001_placeholder::Migration),
            Box::new(m20250513_000001_model_provider::Migration),
        ]
    }
}
