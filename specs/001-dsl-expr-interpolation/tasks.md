# Tasks: 001-dsl-expr-interpolation

**Input**: Design documents from `specs/001-dsl-expr-interpolation/`  
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/interpolation.md

**Organization**: Tasks are grouped by user story (P1/P2/P3) for independent implementation and validation.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 = P1 Engine 支持表达式插值, US2 = P2 错误与可观测性, US3 = P3 文档与帮助
- All paths relative to repository root: `g:\rust\VoxStage` (or equivalent)

---

## Phase 1: Setup

**Purpose**: Verify environment and existing codebase state

- [ ] T001 Verify workspace builds and existing engine tests pass: `cargo build -p vox-engine && cargo test -p vox-engine` at repo root

---

## Phase 2: Foundational (Blocking for US1)

**Purpose**: Prerequisites that all interpolation logic depends on

- [ ] T002 Implement helper to extract `${...}` content with matching `}` (respecting `"` and `'` inside) in `crates/engine/src/lib.rs`; return segment iterator or (literal, expr_inner) pairs for use by interpolate_text

---

## Phase 3: User Story 1 – Engine 层支持 `${表达式}` (P1) MVP

**Goal**: Speak/BGM 文本中 `${...}` 内按表达式解析并求值后拼接；保持 `${name}` 兼容；失败返回错误。

**Independent Test**: Run script with `speak God "第 ${i} 次"` and `speak God "下一项 ${i + 1}"`; confirm output is interpolated correctly; run with invalid expression and confirm error is returned (not silent).

### Implementation for User Story 1

- [ ] T003 [US1] Replace current `interpolate_text` body: for each `${...}` segment use T002 helper to get inner string, then `dsl::parse_expr_from_str(line_idx, inner)` and `eval_expr(&expr, vars)`, then existing value-to-string; append to result. Keep literal segments unchanged. In `crates/engine/src/lib.rs`
- [ ] T004 [US1] Change `interpolate_text` signature to return `Result<String, E>` where `E` carries at least message and optional line/context; on parse or eval failure return `Err` with that context. In `crates/engine/src/lib.rs`
- [ ] T005 [US1] Add interpolation error variant to engine error type (if not already); update BGM path interpolation call site (~line 396) to handle `Result` and propagate error. In `crates/engine/src/lib.rs`
- [ ] T006 [US1] Update speak text interpolation call site (~line 525) to handle `Result` and propagate error so runner/GUI can surface it. In `crates/engine/src/lib.rs`

**Checkpoint**: User Story 1 done when speak/BGM interpolation supports full expressions and returns errors on failure.

---

## Phase 4: User Story 2 – 错误与可观测性 (P2)

**Goal**: 解析/求值失败时错误信息可被 runner/GUI 展示（行号、片段、原因）。

**Independent Test**: Trigger parse or eval error (e.g. `${1/0}` or `${syntax(`); confirm error payload includes readable message and optional line/snippet where applicable.

### Implementation for User Story 2

- [ ] T007 [US2] Ensure interpolation error type includes optional line index and snippet (e.g. the `${...}` inner or full line) so runner/GUI can display it; document in `specs/001-dsl-expr-interpolation/contracts/interpolation.md` if needed. In `crates/engine/src/lib.rs` (and any error enum definition)
- [ ] T008 [US2] Where engine is invoked from Tauri/CLI, ensure interpolation errors are mapped to user-visible messages or logs (no silent swallow). In `apps/voxstage-gui/src-tauri/` or CLI entry as applicable

**Checkpoint**: User Story 2 done when interpolation failures are observable with clear messages and optional context.

---

## Phase 5: User Story 3 – 文档与帮助 (P3)

**Goal**: README 与 GUI 帮助页中说明文本插值支持 `${表达式}` 并给出示例。

**Independent Test**: Open README and GUI 帮助页，确认有“文本插值”“${表达式}”及示例（如 `${i}`、`${i + 1}`、`${format_time(ts)}`）。

### Implementation for User Story 3

- [ ] T009 [US3] Update root `README.md` with a short “文本插值” section: support for `${表达式}` and examples (e.g. `${name}`, `${i + 1}`, `${format_time(ts)}`)
- [ ] T010 [P] [US3] Update GUI help content (e.g. Help tab or help page component) with same interpolation description and examples. In `apps/voxstage-gui/src/` (exact path per project’s help asset location)

**Checkpoint**: User Story 3 done when docs and help reflect the new interpolation behavior.

---

## Phase 6: Polish & Cross-Cutting

**Purpose**: Tests and final validation

- [ ] T011 [P] Add unit tests for interpolation in `crates/engine`: (1) `${name}` only, (2) `${a + b}`, (3) expression with `"}"` inside string literal, (4) parse error returns Err, (5) eval error returns Err. In `crates/engine/src/lib.rs` (e.g. `#[cfg(test)] mod tests`) or `crates/engine/tests/` if present
- [ ] T012 Run validation from `specs/001-dsl-expr-interpolation/quickstart.md` (manual or script): build, run script with expression interpolations, trigger one error case and confirm message

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: None.
- **Phase 2 (Foundational)**: Depends on Phase 1. Blocks Phase 3.
- **Phase 3 (US1)**: Depends on Phase 2. Core implementation.
- **Phase 4 (US2)**: Depends on Phase 3 (error type and call sites already touched in US1).
- **Phase 5 (US3)**: Can run in parallel with Phase 4 (docs only).
- **Phase 6 (Polish)**: Depends on Phase 3 at least; T012 depends on full implementation.

### User Story Dependencies

- **US1 (P1)**: After T002. No dependency on US2/US3.
- **US2 (P2)**: After US1 (refine error payload and propagation).
- **US3 (P3)**: Independent of US1/US2 (documentation only).

### Parallel Opportunities

- T009 and T010 (US3) can be done in parallel.
- T011 (tests) can be done in parallel with T009/T010 once US1 is done.

---

## Implementation Strategy

### MVP First (US1 only)

1. T001 → T002 → T003 → T004 → T005 → T006.
2. Validate: run script with `${i}` and `${i + 1}`, confirm output; trigger one error, confirm Err.
3. Then add US2 (error payload) and US3 (docs), then Polish (T011, T012).

### Task Count Summary

| Phase   | Task IDs   | Count |
|---------|------------|-------|
| Setup   | T001       | 1     |
| Foundational | T002   | 1     |
| US1 (P1) | T003–T006  | 4     |
| US2 (P2) | T007–T008  | 2     |
| US3 (P3) | T009–T010  | 2     |
| Polish  | T011–T012  | 2     |
| **Total** | **12**   |       |

**Suggested MVP scope**: Phase 1 + Phase 2 + Phase 3 (T001–T006). After that, interpolation is functional and errors are propagated; then add observability (US2), docs (US3), and tests (Polish).
