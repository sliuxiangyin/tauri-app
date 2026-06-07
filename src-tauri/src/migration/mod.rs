pub use sea_orm_migration::prelude::*;

mod m20250512_000001_placeholder;
mod m20250513_000001_model_provider;
mod m20250514_000001_mcp;
mod m20250515_000001_conversations;
mod m20250515_000002_plans;
mod m20250515_000003_messages;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250512_000001_placeholder::Migration),
            Box::new(m20250513_000001_model_provider::Migration),
            Box::new(m20250514_000001_mcp::Migration),
            Box::new(m20250515_000001_conversations::Migration),
            Box::new(m20250515_000002_plans::Migration),
            Box::new(m20250515_000003_messages::Migration),
        ]
    }
}
