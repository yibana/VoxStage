# Implementation Plan: 001-dsl-expr-interpolation

**Branch**: `001-dsl-expr-interpolation` | **Date**: 2026-02-27 | **Spec**: [spec.md](./spec.md)  
**Input**: Feature specification from `specs/001-dsl-expr-interpolation/spec.md`

## Summary

在 VOX DSL 的 speak/BGM 等文本中，将 `${变量}` 扩展为 `${表达式}`：插值内容按现有 DSL 表达式语法解析并求值后转为字符串再拼接。保持对仅变量名 `${name}` 的向后兼容；解析/求值失败时提供明确错误信息（含行号或上下文）。实现集中在 engine 的 `interpolate_text`，复用 dsl 的 `parse_expr_from_str` 与 engine 的 `eval_expr`。

## Technical Context

**Language/Version**: Rust（与现有 Cargo workspace 一致）  
**Primary Dependencies**: 现有 `vox-engine`（依赖 `vox-dsl`）、`vox-dsl` 解析器与表达式 AST  
**Storage**: N/A（无新增持久化）  
**Testing**: `cargo test`（engine 与 dsl 现有测试）；可增加针对 `interpolate_text` 的单元测试（含表达式、嵌套 `}`、错误路径）  
**Target Platform**: 与现有项目一致（Windows/macOS/Linux，CLI + GUI）  
**Project Type**: library（engine）+ 现有 CLI/GUI 无接口变更  
**Performance Goals**: 单次插值开销与现有表达式求值同量级，不引入明显延迟  
**Constraints**: 向后兼容已有 `${name}` 剧本；错误信息可被 runner/GUI 展示  
**Scale/Scope**: 单次 speak 文本内多个 `${...}`，表达式与现有 set/condition 共用语法

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| 宪章原则 | 本 feature 符合情况 |
|----------|---------------------|
| I. 剧本优先 & 语法兼容 | 扩展插值为表达式，不改变现有语法；仅变量名形式仍合法，向后兼容。 |
| II. Engine / Runner / GUI 分层 | 改动仅在 engine 层（`interpolate_text`）；runner/GUI 无接口变更，仅享受更丰富插值能力。 |
| III. 可观测性 | 解析/求值失败时返回或上报明确错误（行号/片段/原因），便于日志与前端展示。 |
| IV. Provider 与配置驱动 | 不涉及。 |
| V. 简单与资源受控 | 复用现有解析与求值，无新增外部依赖；单次插值内存与现有表达式求值同量级。 |

**Verdict**: 无违规，可进入 Phase 0/1。

## Project Structure

### Documentation (this feature)

```text
specs/001-dsl-expr-interpolation/
├── plan.md              # 本文件
├── spec.md              # 需求说明
├── research.md          # Phase 0 输出
├── data-model.md        # Phase 1 输出
├── quickstart.md        # Phase 1 输出
├── contracts/           # Phase 1 输出（插值语法/错误契约）
└── tasks.md             # Phase 2 输出（/speckit.tasks 生成）
```

### Source Code (repository root)

```text
crates/
├── dsl/                 # 表达式解析（parse_expr_from_str），本 feature 仅使用，不修改
│   └── src/parser.rs
└── engine/              # 本 feature 主要改动
    └── src/lib.rs       # interpolate_text 改为“提取 ${...} 内容 → 解析表达式 → 求值 → 转字符串”

apps/
└── voxstage-gui/        # 无代码改动；文档/帮助更新在 P3
```

**Structure Decision**: 单仓库 Cargo workspace；本 feature 仅修改 `crates/engine` 中插值逻辑，并可选在 `crates/engine` 或 `crates/dsl` 的 tests 中增加用例。

## Complexity Tracking

无宪章违规需要豁免。
