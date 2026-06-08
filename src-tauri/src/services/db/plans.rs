//! Plans 表 CRUD 操作

use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
};

#[allow(unused_imports)]
use crate::entity::plans::{self as plans_entity, ActiveModel, Model as PlanModel};
use crate::entity::plans::{CreatePlanPayload, UpdatePlanPayload};

/// 生成唯一 ID（使用 UUID v7）
fn generate_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// 保存 Plan（创建新记录）
pub async fn save_plan(
    db: &DatabaseConnection,
    payload: CreatePlanPayload,
) -> Result<String, String> {
    let now = chrono::Utc::now().timestamp();

    let active_model = ActiveModel {
        id: Set(generate_id()),
        mid: Set(payload.mid),
        need_agent: Set(if payload.need_agent { "true" } else { "false" }.to_string()),
        order_num: Set(payload.order_num),
        reasoning: Set(payload.reasoning),
        steps: Set(payload.steps),
        step_results: Set(payload.step_results),
        stop_reason: Set(payload.stop_reason),
        completed_at: Set(payload.completed_at),
        created_at: Set(now),
    };

    let model = active_model.insert(db).await.map_err(|e| e.to_string())?;
    Ok(model.id)
}

/// 根据 mid 获取最新的 Plan
#[allow(dead_code)]
pub async fn get_plan_by_mid(
    db: &DatabaseConnection,
    mid: String,
) -> Result<Option<plans_entity::Model>, String> {
    let plan = plans_entity::Entity::find()
        .filter(plans_entity::Column::Mid.eq(&mid))
        .one(db)
        .await
        .map_err(|e| e.to_string())?;

    Ok(plan)
}

/// 根据 id 获取 Plan
#[allow(dead_code)]
pub async fn get_plan_by_id(
    db: &DatabaseConnection,
    plan_id: String,
) -> Result<Option<plans_entity::Model>, String> {
    let plan = plans_entity::Entity::find_by_id(plan_id)
        .one(db)
        .await
        .map_err(|e| e.to_string())?;

    Ok(plan)
}

/// 更新 Plan
pub async fn update_plan(
    db: &DatabaseConnection,
    plan_id: String,
    payload: UpdatePlanPayload,
) -> Result<(), String> {
    let plan = plans_entity::Entity::find_by_id(plan_id.clone())
        .one(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Plan not found: {}", plan_id))?;

    let mut active_model: ActiveModel = plan.into();

    if let Some(step_results) = payload.step_results {
        active_model.step_results = Set(Some(step_results));
    }
    if let Some(stop_reason) = payload.stop_reason {
        active_model.stop_reason = Set(Some(stop_reason));
    }
    if let Some(completed_at) = payload.completed_at {
        active_model.completed_at = Set(Some(completed_at));
    }

    active_model.update(db).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除 Plan（根据 mid）
#[allow(dead_code)]
pub async fn delete_plan_by_mid(db: &DatabaseConnection, mid: String) -> Result<u64, String> {
    let result = plans_entity::Entity::delete_many()
        .filter(plans_entity::Column::Mid.eq(&mid))
        .exec(db)
        .await
        .map_err(|e| e.to_string())?;

    Ok(result.rows_affected)
}