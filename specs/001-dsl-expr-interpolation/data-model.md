# Data Model: 001-dsl-expr-interpolation

本 feature 不新增持久化实体，仅扩展 engine 内「文本插值」的语义。以下为概念模型与现有类型的使用约定。

## 现有类型（沿用）

| 类型 | 所在 crate | 说明 |
|------|------------|------|
| `Expr` | vox-dsl | 表达式 AST，由 `parse_expr_from_str` 产生。 |
| `Value` | vox-engine | 求值结果（String / Num），由 `eval_expr` 产生。 |
| `HashMap<String, String>` | vox-engine | 变量表；key 为变量名，value 为字符串形式。 |

## 插值过程（概念）

1. **输入**: 原始文本 `text: &str`，以及变量表 `vars`。
2. **分段**: 将 `text` 拆分为交替的「字面量片段」与「`${...}` 表达式片段」；每个 `${...}` 用「匹配的 `}` 边界」提取出内容子串 `inner`。
3. **解析**: 对每个 `inner` 调用 dsl 的 `parse_expr_from_str(line_idx, inner)` 得到 `Expr`。
4. **求值**: 对每个 `Expr` 调用 engine 的 `eval_expr(&expr, vars)` 得到 `Value`。
5. **转字符串**: 将 `Value` 转为字符串（与现有 `value_to_string` 或等价逻辑一致）。
6. **输出**: 按顺序拼接字面量与求值结果，得到最终 `String`。

任一解析或求值步骤失败则返回错误（见 contracts），不静默保留原样。

## 状态与校验

- 无新增持久化状态。
- 校验规则：`inner` 须为合法 DSL 表达式；求值结果须可转为字符串（当前 `Value` 已支持）。
