use crate::db::DbState;

#[tauri::command]
pub async fn db_health_check(state: tauri::State<'_, DbState>) -> Result<String, String> {
    state.get().await.map_err(|e| e.to_string())?;
    Ok("ok".into())
}
