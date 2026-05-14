import { invoke } from "@tauri-apps/api/core";

import type { ModelConfigProfile, ModelEntry, ProviderKind } from "@/components/model-config";

// ---------------------------------------------------------------------------
// Backend DTOs (snake_case, matching Rust structs in model_config.rs)
// ---------------------------------------------------------------------------

interface BackendProviderConfigDto {
  id: string;
  display_name: string;
  enabled: boolean;
  provider_kind: string;
  api_base_url: string;
  api_key: string | null;
  extra_json: string | null;
  sort_index: number;
  created_at: string | null;
  updated_at: string | null;
}

interface BackendProviderModelDto {
  id: string;
  config_id: string;
  model_id: string;
  model_name: string;
  group_name: string;
  sort_index: number;
}

/** `ProviderConfigWithModels`: the config fields are flattened into the payload. */
interface BackendConfigWithModels extends BackendProviderConfigDto {
  models: BackendProviderModelDto[];
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

function backendToProfile(b: BackendConfigWithModels): ModelConfigProfile {
  return {
    id: b.id,
    displayName: b.display_name,
    enabled: b.enabled,
    provider: b.provider_kind as ProviderKind,
    apiBaseUrl: b.api_base_url,
    apiKey: b.api_key ?? "",
    models: b.models.map(
      (m): ModelEntry => ({
        id: m.id,
        modelId: m.model_id,
        modelName: m.model_name,
        groupName: m.group_name,
      }),
    ),
  };
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/** 从后端加载全部配置及其模型。 */
export async function fetchAllConfigs(): Promise<ModelConfigProfile[]> {
  const raw: BackendConfigWithModels[] = await invoke("list_provider_configs");
  return raw.map(backendToProfile);
}

/** 新建配置（含模型）；返回与前端一致的 profile（已落库）。 */
export async function createConfig(
  profile: ModelConfigProfile,
): Promise<ModelConfigProfile> {
  await invoke("create_provider_config", {
    payload: {
      id: profile.id,
      display_name: profile.displayName,
      provider_kind: profile.provider,
      api_base_url: profile.apiBaseUrl,
      api_key: profile.apiKey || null,
      extra_json: null,
    },
  });

  // Upsert all models for this config
  for (let i = 0; i < profile.models.length; i++) {
    const m = profile.models[i];
    await invoke("upsert_provider_model", {
      payload: {
        id: m.id,
        config_id: profile.id,
        model_id: m.modelId,
        model_name: m.modelName,
        group_name: m.groupName,
        sort_index: i,
      },
    });
  }

  return profile;
}

/** 更新已有配置并同步模型变更。
 *
 * `previousModelIds` 应传入本次编辑**之前**该配置下的所有模型 ID 集合；
 * 函数会对比当前模型列表，删除已移除的行，upsert 剩余行。 */
export async function updateConfig(
  profile: ModelConfigProfile,
  previousModelIds: Set<string>,
): Promise<void> {
  await invoke("update_provider_config", {
    id: profile.id,
    payload: {
      display_name: profile.displayName,
      enabled: profile.enabled,
      provider_kind: profile.provider,
      api_base_url: profile.apiBaseUrl,
      api_key: profile.apiKey || null,
      extra_json: null,
    },
  });

  // Diff models: delete removed, upsert current
  const currentIds = new Set(profile.models.map((m) => m.id));

  for (const oldId of previousModelIds) {
    if (!currentIds.has(oldId)) {
      await invoke("delete_provider_model", { id: oldId });
    }
  }

  for (let i = 0; i < profile.models.length; i++) {
    const m = profile.models[i];
    await invoke("upsert_provider_model", {
      payload: {
        id: m.id,
        config_id: profile.id,
        model_id: m.modelId,
        model_name: m.modelName,
        group_name: m.groupName,
        sort_index: i,
      },
    });
  }
}

/** 删除配置（级联删除其下所有模型）。 */
export async function deleteConfigById(id: string): Promise<void> {
  await invoke("delete_provider_config", { id });
}
