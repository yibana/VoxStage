/** 与 Rust AppConfig 对应 */

export interface ModelEntry {
  name: string;
  type: string;
  provider: string;
  endpoint: string;
  model_id: string;
  extra: Record<string, string>;
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
  };
}

export function emptyRole(): RoleEntry {
  return { name: "", model: "", params: {} };
}
