use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "model_provider_model")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub config_id: String,
    pub model_id: String,
    pub model_name: String,
    pub group_name: String,
    pub sort_index: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::model_provider_config::Entity",
        from = "Column::ConfigId",
        to = "super::model_provider_config::Column::Id",
        on_delete = "Cascade"
    )]
    Config,
}

impl Related<super::model_provider_config::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Config.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
