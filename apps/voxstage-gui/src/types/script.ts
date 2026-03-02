/** 剧本列表项类型定义（仅用于前端编排，后续可映射到 DSL AST）。 */

export type ScriptItemType =
  | "speak"
  | "sleep"
  | "if"
  | "for"
  | "while"
  | "let"
  | "set"
  | "bgm_play"
  | "bgm_volume"
  | "bgm_pause"
  | "bgm_resume"
  | "bgm_stop";

export interface ScriptItem {
  id: string;
  type: ScriptItemType;
  /** 缩进层级，用于列表显示（0 为顶层）。 */
  indent: number;

  /** 静态语句索引（与 EngineCommand source_index 对齐），用于运行进度高亮。 */
  sourceIndex?: number;

  /** speak: 角色名（来自全局角色配置） */
  role?: string;
  /** speak: 文本 */
  text?: string;
  /** speak: 本句覆写的角色参数（key/value），仅当前 speak 生效 */
  speakParams?: Record<string, string>;

  /** sleep: 毫秒 */
  ms?: number;

  /** if / while: 条件表达式（字符串） */
  condition?: string;

  /** for: 次数表达式（字符串） */
  times?: string;

  /** let / set: 变量名 */
  varName?: string;
  /** let / set: 表达式字符串 */
  expr?: string;

  /** bgm_play: 路径或 URL */
  bgmPath?: string;
  /** bgm_play: 是否循环 */
  bgmLoop?: boolean;

  /** bgm_volume: 音量（0.0 - 1.0） */
  bgmVolume?: number;
}

let nextId = 1;

export function createItem(type: ScriptItemType, indent = 0): ScriptItem {
  const id = `item-${nextId++}`;
  switch (type) {
    case "speak":
      return { id, type, indent, role: "", text: "", speakParams: {} };
    case "sleep":
      return { id, type, indent, ms: 1000 };
    case "if":
      return { id, type, indent, condition: "" };
    case "for":
      return { id, type, indent, times: "1" };
    case "while":
      return { id, type, indent, condition: "" };
    case "let":
      return { id, type, indent, varName: "", expr: "" };
    case "set":
      return { id, type, indent, varName: "", expr: "" };
    case "bgm_play":
      return { id, type, indent, bgmPath: "", bgmLoop: true };
    case "bgm_volume":
      return { id, type, indent, bgmVolume: 0.5 };
    case "bgm_pause":
    case "bgm_resume":
    case "bgm_stop":
      return { id, type, indent };
  }
}

/** 简单转义字符串中的 `\` 与 `\"`，用于文本写入 .vox 脚本。 */
function escapeString(input: string): string {
  return input.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

import type { AppConfig } from "./config";

/** 将当前配置与剧本步骤导出为 .vox 文本（不负责写文件）。 */
export function toVox(config: AppConfig, items: ScriptItem[]): string {
  const lines: string[] = [];

  // model 块
  for (const m of config.models) {
    if (!m.name) continue;
    lines.push(`model ${m.name} {`);
    if (m.type) lines.push(`  type = "${escapeString(m.type)}"`);
    if (m.provider) lines.push(`  provider = "${escapeString(m.provider)}"`);
    if (m.endpoint) lines.push(`  endpoint = "${escapeString(m.endpoint)}"`);
    if (m.model_id) lines.push(`  model_id = "${escapeString(m.model_id)}"`);
    for (const [k, v] of Object.entries(m.extra ?? {})) {
      if (!k) continue;
      lines.push(`  ${k} = "${escapeString(String(v))}"`);
    }
    lines.push("}");
    lines.push("");
  }

  // role 块
  for (const r of config.roles) {
    if (!r.name) continue;
    lines.push(`role ${r.name} {`);
    if (r.model) lines.push(`  model = ${r.model}`);
    for (const [k, v] of Object.entries(r.params ?? {})) {
      if (!k) continue;
      lines.push(`  ${k} = "${escapeString(String(v))}"`);
    }
    lines.push("}");
    lines.push("");
  }

  // 剧本语句：根据 indent 模拟块结构
  let currentDepth = 0;
  const pad = (depth: number) => "  ".repeat(depth);

  for (const it of items) {
    const targetIndent = Math.max(0, it.indent ?? 0);

    // 关闭多余块
    while (currentDepth > targetIndent) {
      currentDepth -= 1;
      lines.push(`${pad(currentDepth)}}`);
    }

    const d = targetIndent;

    switch (it.type) {
      case "let": {
        if (!it.varName) break;
        const expr = it.expr ?? "";
        lines.push(`${pad(d)}let ${it.varName} = ${expr}`);
        break;
      }
      case "set": {
        if (!it.varName) break;
        const expr = it.expr ?? "";
        lines.push(`${pad(d)}set ${it.varName} = ${expr}`);
        break;
      }
      case "speak": {
        const role = it.role ?? "";
        const text = escapeString(it.text ?? "");
        if (!role && !text) break;
        const params = it.speakParams ?? {};
        const paramEntries = Object.entries(params).filter(
          ([k, v]) => k && v != null && String(v).length > 0,
        );
        let paramPart = "";
        if (paramEntries.length > 0) {
          const sorted = paramEntries.sort(([a], [b]) => a.localeCompare(b));
          const inner = sorted
            .map(([k, v]) => `${k} = "${escapeString(String(v))}"`)
            .join(", ");
          paramPart = `(${inner})`;
        }
        lines.push(`${pad(d)}speak ${role}${paramPart} "${text}"`);
        break;
      }
      case "sleep": {
        const ms = it.ms ?? 0;
        lines.push(`${pad(d)}sleep ${ms}`);
        break;
      }
      case "if": {
        const cond = it.condition ?? "";
        lines.push(`${pad(d)}if ${cond} {`);
        currentDepth = d + 1;
        break;
      }
      case "for": {
        const times = it.times ?? "1";
        lines.push(`${pad(d)}for ${times} {`);
        currentDepth = d + 1;
        break;
      }
      case "while": {
        const cond = it.condition ?? "";
        lines.push(`${pad(d)}while ${cond} {`);
        currentDepth = d + 1;
        break;
      }
      case "bgm_play": {
        const path = it.bgmPath ?? "";
        if (!path) break;
        const escaped = escapeString(path);
        const loop = it.bgmLoop ?? true;
        const loopSuffix = loop ? " loop" : "";
        lines.push(`${pad(d)}bgm "${escaped}"${loopSuffix}`);
        break;
      }
      case "bgm_volume": {
        const vol = it.bgmVolume ?? 1.0;
        lines.push(`${pad(d)}bgm_volume ${vol}`);
        break;
      }
      case "bgm_pause": {
        lines.push(`${pad(d)}bgm_pause`);
        break;
      }
      case "bgm_resume": {
        lines.push(`${pad(d)}bgm_resume`);
        break;
      }
      case "bgm_stop": {
        lines.push(`${pad(d)}bgm_stop`);
        break;
      }
    }
  }

  // 关闭所有未闭合块
  while (currentDepth > 0) {
    currentDepth -= 1;
    lines.push(`${pad(currentDepth)}}`);
  }

  return lines.join("\n");
}


