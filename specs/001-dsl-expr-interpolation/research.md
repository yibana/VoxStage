# Research: 001-dsl-expr-interpolation

## 1. `${...}` 内容提取（含嵌套 `}`）

**Decision**: 从 `${` 开始，按「括号/引号匹配」找到对应的结束 `}`，再对中间子串做表达式解析。

**Rationale**:
- 表达式内可能出现字符串字面量（如 `"}"` 或 `'}'`），若仅用“遇到第一个 `}` 就截断”会错误截断。
- 已有 DSL 表达式语法包含 `"..."`、`'...'` 与括号；不引入新语法，只需在 engine 侧实现一个“找匹配 `}`”的扫描器：遇到 `"` 或 `'` 则跳到下一个引号，遇到 `{` 深度+1、`}` 深度-1，深度归 0 时停止。

**Alternatives considered**:
- 正则：难以可靠处理引号内 `}`，故不采用。
- 在 dsl 层增加“插值片段”专用解析：可行但本 feature 范围仅 engine 层，且当前表达式已支持所有需要的语法，故优先在 engine 内用简单扫描提取子串。

## 2. 解析/求值失败时的行为

**Decision**: 失败时返回 `Result`（或通过回调/错误类型上报），携带行号（或调用方传入的上下文）与原因；不在 engine 内静默保留 `${...}` 原样，由调用方决定是否回退显示或打日志。

**Rationale**:
- 宪章要求可观测性：错误信息需明确，便于 runner/GUI 展示与调试。
- 若 engine 静默保留原样，用户难以发现脚本错误。

**Alternatives considered**:
- 静默保留原样并打日志：可观测性不足，不采纳为默认。
- 配置项“失败时保留原样”：可后续迭代，本 feature 先做“失败即错误”。

## 3. 与现有 `eval_expr` / 变量表类型一致

**Decision**: 继续使用 engine 现有 `eval_expr(expr, vars)` 与 `HashMap<String, String>` 变量表；插值处与 set/condition 等共用同一套求值逻辑与内置函数。

**Rationale**:
- 现有代码已满足需求，无需引入新类型或新求值路径。
- 内置函数由 `eval_builtin` 统一处理，插值中 `${format_time(ts)}` 等自然可用。

**Alternatives considered**:
- 插值专用求值器：重复逻辑，不采纳。
