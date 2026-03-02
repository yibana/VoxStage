/** 与 Rust AppConfig 对应 */

export interface ModelEntry {
  name: string;
  type: string;
  provider: string;
  endpoint: string;
  model_id: string;
  extra: Record<string, string>;
  /** 是否启用该模型的音频缓存（相同文本/参数复用合成结果） */
  enable_cache: boolean;
}

export interface RoleEntry {
  name: string;
  model: string;
  params: Record<string, string>;
}

export interface AppConfig {
  models: ModelEntry[];
  roles: RoleEntry[];
}

export function emptyModel(): ModelEntry {
  return {
    name: "",
    type: "http",
    provider: "",
    endpoint: "",
    model_id: "",
    extra: {},
    enable_cache: false,
  };
}

export function emptyRole(): RoleEntry {
  return { name: "", model: "", params: {} };
}
