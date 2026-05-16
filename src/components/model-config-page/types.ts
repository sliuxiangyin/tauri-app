export type ProviderKind = "open_ai" | "ollama";

export type ModelEntry = {
  id: string;
  modelId: string;
  modelName: string;
  groupName: string;
};

export type ModelConfigProfile = {
  id: string;
  displayName: string;
  enabled: boolean;
  provider: ProviderKind;
  apiBaseUrl: string;
  apiKey: string;
  models: ModelEntry[];
};

export function newId(): string {
  return crypto.randomUUID();
}

export function createEmptyModelEntry(): ModelEntry {
  return {
    id: newId(),
    modelId: "",
    modelName: "",
    groupName: "",
  };
}

export function createDefaultProfile(): ModelConfigProfile {
  return {
    id: newId(),
    displayName: "",
    enabled: true,
    provider: "open_ai",
    apiBaseUrl: "",
    apiKey: "",
    models: [],
  };
}

export function listItemLabel(p: ModelConfigProfile): string {
  const name = p.displayName.trim();
  if (name) return name;
  const url = p.apiBaseUrl.trim();
  if (url) return url;
  return "未命名配置";
}

/** 模型行是否满足必填：模型 ID、模型名称非空（去空白）。 */
export function isModelEntryValid(m: ModelEntry): boolean {
  return m.modelId.trim().length > 0 && m.modelName.trim().length > 0;
}

/** 配置基础必填：显示名称、API 地址非空（去空白）。 */
export function isModelConfigProfileCoreValid(p: ModelConfigProfile): boolean {
  return p.displayName.trim().length > 0 && p.apiBaseUrl.trim().length > 0;
}
