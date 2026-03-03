# Quickstart: 验证 001-dsl-expr-interpolation

## 手工验证步骤

1. **构建**: 在仓库根目录执行 `cargo build -p vox-engine`（或通过 GUI/CLI 的完整构建）。
2. **准备脚本**: 创建或使用已有 `.vox` 脚本，其中 speak 行包含：
   - 仅变量：`speak God "你好，${name}"`
   - 表达式：`speak God "第 ${i} 次，下一项是 ${i + 1}"`
   - 内置函数：`speak God "时间：${format_time(ts)}"`
3. **运行**: 使用 CLI 或 GUI 执行该脚本，确认输出语音/文本为插值后的结果。
4. **错误路径**: 故意写非法表达式（如 `${1/0}` 或 `${undefined_fn()}`），确认能收到明确错误信息而非静默错误或崩溃。

## 单元测试（建议）

- 在 `crates/engine` 中为 `interpolate_text`（或新的 `interpolate_text_expr`）增加测试用例：
  - `${name}` → 变量替换；
  - `${a + b}` → 算术结果；
  - `${"}"}` 或类似含 `}` 的字符串字面量 → 正确匹配结束括号；
  - 解析失败 / 求值失败 → 返回 `Err` 且包含可读信息。
