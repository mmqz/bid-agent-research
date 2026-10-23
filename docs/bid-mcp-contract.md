# 标书业务 MCP 工具契约（bid-mcp-contract）

> 定位：Agent 经 MCP 调用标书业务能力的**唯一契约层**。本阶段只做契约设计，不实现。
> 工具数量：**12 个**（8 个核心业务工具 + 4 个辅助工具）。刻意保持小工具面——审计证实：MCP 只挂示例工具没有业务价值（tender-cli R-38/§1.12），工具越多低能力模型的误用率越高（详见 §7）。
> 每个工具的语义均直接对应 `docs/bid-domain-model.md` §6 状态机与 §2 实体。

## 0. 通用约定

- **传输**：MCP stdio / streamable-http；工具即业务操作，不暴露 SQL/文件级细粒度。
- **公共参数**：变更类工具必带 `idempotency_key`（24h 内重复提交返回首次响应，含原返回体）；所有工具带 `project_id`（除 create_bid_project）。
- **统一返回包**：成功 `{ok:true, data:{...}}`；失败 `{ok:false, error:{code, message, details, retry_class, agent_hint}}`。`agent_hint` 是给模型的自救指引一句话（如"请先调用 get_bid_project_state 查看允许的操作"）。
- **状态前置校验**：每个工具在服务端校验项目状态，不满足即返回 `INVALID_STATE`（附 `current_status` 与 `allowed_next_tools`）——低能力模型可据此自我纠正，这是从 tender-cli / Get00 断链教训（risk-register S-6）反推出的硬性设计。
- **所有变更均写 ProjectEvent**（事件溯源，见 domain-model §8）。

## 1. 工具总表

| # | 工具 | 类型 | 前置状态 | 触发转换 | 幂等性 |
|---|------|------|---------|---------|--------|
| 1 | create_bid_project | 变更 | — | →CREATED | external_ref + idempotency_key |
| 2 | import_tender_files | 变更(异步) | CREATED/PARSED/REQUIREMENTS_READY(补遗) | →FILES_IMPORTING→PARSED/PARSE_FAILED | sha256 去重 + idempotency_key |
| 3 | extract_requirements | 变更(异步) | PARSED / REQUIREMENTS_READY(补遗增量) | →REQUIREMENTS_READY | (project, scope, 快照哈希) |
| 4 | get_compliance_matrix | 只读 | ≥REQUIREMENTS_READY | — | 纯读 |
| 5 | search_evidence | 只读 | ≥CREATED | — | 纯读 |
| 6 | draft_response_section | 变更 | OUTLINE_APPROVED+ 且 SEC∈{queued,drafted,revision} | SEC 子状态机 | (section, expected_version) |
| 7 | validate_bid_document | 变更 | SECTIONS_DRAFTED | →VALIDATED | (project, 快照 CAS 集) |
| 8 | export_bid_document | 变更(异步) | READY_FOR_EXPORT | →EXPORTING→EXPORTED | idempotency_key |
| 9 | get_bid_project_state | 只读 | 任意 | —（恢复入口） | 纯读 |
| 10 | attach_evidence | 变更 | ≥REQUIREMENTS_READY | 写 EVL，不转移状态 | (evidence,target,quote_hash) |
| 11 | record_review_decision | 变更 | 人审门开放 | 过门（见状态机） | (gate_id, project) 一次决策 |
| 12 | create_revision_task | 变更 | 存在 open FIND | REV 任务入队 | (finding_ids, section) |

## 2. 核心工具契约

### 2.1 create_bid_project —— 创建项目
**语义**：创建标书项目；`external_ref` 已存在则幂等返回原项目（`existing=true`），保证上游重试安全。

| 参数 | 类型 | 必填 | 校验 |
|------|------|------|------|
| name | string 1..200 | ✓ | 非空 |
| external_ref | string ≤200 | | 唯一性幂等键 |
| meta | object | | `tenderer`, `deadline`(ISO8601), `bid_mode`∈{open,blind} |
| idempotency_key | string | ✓ | |

**返回**：`{project_id, status:"CREATED", existing:false}`
**错误**：`INVALID_ARGUMENT`（PERMANENT）／`NAME_REQUIRED`（NEEDS_INPUT）
**状态**：创建后 `CREATED`；重复调用不产生新事件（幂等命中直接返回）。

### 2.2 import_tender_files —— 导入并解析招标文件
**语义**：注册文件（sha256 去重）→ 解析（结构化分块）→ 完整性对账。补遗（role=addendum）在 REQUIREMENTS_READY 后仍可追加（触发增量失效传播，见 domain-model §5）。异步作业：返回 import_job_id，经 get_bid_project_state 轮询。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| files[] | array | ✓ | {file_ref, filename, role∈{main,attachment,addendum,clarification}, supersedes?} |
| mode | enum | | `append`(默认)/`replace`(仅 CREATED) |
| idempotency_key | string | ✓ | |

**返回**：`{import_job_id, files:[{file_id, sha256, parse_status, page_count, chunk_count, deduped, ocr_required}]}`（解析完成后经状态查询取全量结果）
**守卫（完整性对账，防截断丢条款——risk-register S-1）**：`Σchunk.char_count ≥ 0.98 × 提取文本量` 且每页至少归入一个 chunk；对账失败 → `PARSE_INTEGRITY_FAILED`（NEEDS_INPUT，附 coverage 报告）。
**错误**：`UNSUPPORTED_FORMAT`（PERMANENT）／`NEEDS_OCR`（NEEDS_INPUT：文件置 needs_ocr，提示先走 OCR 子任务）／`PARSE_INTEGRITY_FAILED`（NEEDS_INPUT）／`FILE_HASH_MISMATCH`（PERMANENT）／`INVALID_STATE`
**状态**：`CREATED→FILES_IMPORTING→PARSED`；失败 `PARSE_FAILED`（修复后重入）。

### 2.3 extract_requirements —— 抽取六类条款
**语义**：对 parsed 文件分块执行六类条款抽取（双通道：LLM 结构化抽取 + 规则兜底互验，ark 思路），产出 REQ（ID=hash(规范文本+锚点)，重跑稳定）并逐项**覆盖率对账**。补遗增量：仅对新增/变更文件执行，与既有 REQ 清单做内容对齐（命中→superseded 血缘，未命中→新增）。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| scope | enum/all | | `all` 或 `file_ids[]` |
| options | object | | `dual_channel:true`(默认) |
| idempotency_key | string | ✓ | |

**返回**：`{job_id, summary:{total, per_category:{qualification,rejection,scoring,technical,commercial,format}, coverage:{chunk_count, covered_chunks, uncovered:[{chunk_id,page}], }}, quality_flags[]}`（quality_flags：如"评分项未逐项拆分"——低置信结果显式暴露，禁止静默丢弃）
**守卫**：①六类枚举封闭，分类置信不足 → 该块重试（不产生"其他"）；②`rejection` 类数量为 0 时产生 `quality_flag: NO_REJECTION_CLAUSE`（NEEDS_INPUT——多数真实招标必有否决条款，零否决项通常是抽取失败信号）。
**错误**：`PARSE_INCOMPLETE`（存在 needs_ocr 文件）／`COVERAGE_CHECK_FAILED`（NEEDS_INPUT，附未覆盖块清单）／`INVALID_STATE`
**状态**：`PARSED→REQUIREMENTS_READY`。

### 2.4 get_compliance_matrix —— 读取合规矩阵
**语义**：只读。返回 要求×证据×章节 合规视图，供 Agent/人 审阅与人审门输入。

| 参数 | 类型 | 必填 |
|------|------|------|
| filter | object | {category[], response_status[], blocked_only, requirement_id, section_id} |
| page | object | {cursor, limit≤100} |

**返回**：`{items:[{requirement_id, category, normalized_text, origin:{file,page,quote}, compliance:{id, response_status, decided_by, blocked, waiver}, evidence:[{evidence_id, quote, status, valid_until}], section_id}], summary:{counts, blocking_count}}`
**错误**：`INVALID_STATE`（未到 REQUIREMENTS_READY，附 allowed_next_tools）
**状态**：不转移；首次全量构建发生在 extract 完成后的内部服务（或在首次调用时惰性构建 → 构建完成即 `MATRIX_READY`）。

### 2.5 search_evidence —— 证据检索（三通道）
**语义**：在企业证据库执行**混合检索**：向量语义 + FTS/trgm 词法 + **结构化字段过滤器**（证号精确 / 金额区间 / 日期有效期 / 型号归一）。语义与精确必须兼得——审计证实 15 仓库仅 ark/ai_bidding 接近双通道（十问 Q4）。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| query.text | string | 二选一 | 语义+词法通道 |
| filters | object | 二选一 | kind[], cert_no(精确), model_no, amount{min,max}, valid_on(date), date_range |
| mode | enum | | semantic/exact/hybrid(默认) |
| include_expired | bool | | 默认 false——过期证据默认不返回（ark 硬过滤吸收） |
| limit/cursor | | | limit≤50 |

**返回**：`{results:[{evidence_id, kind, title, snippet, fields:{cert_no,amount,valid_until,...}, status, score:{semantic,lexical,field_bonus}}], query_digest}`
**错误**：`QUERY_REQUIRED`（text 与 filters 均空）／`INVALID_ARGUMENT`
**状态**：无前置（CREATED 起可用，供人工预检）；纯读。

### 2.6 draft_response_section —— 单章生成/修订（任务化）
**语义**：为一个章节创建 SectionTask 并执行：服务端组装**有界上下文包**（该章 requirement 摘要 + 匹配证据 EVL + 大纲注记 + 风格策略 + 字数目标——服务端组包，避免把全量招标文件塞给模型），生成正文 → 写入内核块（经适配器 CAS）→ **生成后引用回查**（正文中每条证据引文必须能在 EVL.quote 中命中，risk-register P-2）。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| section_id | SEC-ID | ✓ | |
| mode | enum | | draft(默认)/revise |
| instruction | string | revise 必填 | 修订指令（人审意见/finding 摘要） |
| expected_version | int | revise 必填 | CAS：基于的章节版本 |
| idempotency_key | string | ✓ | |

**返回**：`{task_id, section:{section_id, status, version}, content_digest, evidence_used:[EVL...], token_usage:{prompt,completion,cost}}`
**行为细节**：
- 强制要求项（requirement_ids）中存在未满足且未豁免的 blocking CMP → 返回 `EVIDENCE_MISSING` 并把该 SEC 置 `blocked`（十问 Q5：绝不带病生成）。
- 生成后引用回查失败 → 任务失败 + FIND(blocking, rule=CITATION_MISMATCH) + ATTEMPTS 计数；`attempts>max_attempts` → `ATTEMPTS_EXCEEDED`（NEEDS_INPUT：转人工）。
- 上下文包 server-side 构造并持久化（context_bundle_ref），重试复用同一包（结果可复现、成本可审计）。
**错误**：`INVALID_STATE`／`SECTION_NOT_FOUND`／`EVIDENCE_MISSING`（NEEDS_INPUT）／`CITATION_MISMATCH`（先 SAFE_RETRY 1 次，再 NEEDS_INPUT）／`ATTEMPTS_EXCEEDED`（NEEDS_INPUT）／`MODEL_TIMEOUT`/`MODEL_OVERLOAD`（SAFE_RETRY，服务端指数退避）／`VERSION_CONFLICT`（NEEDS_INPUT：重读状态后重试）
**状态**：SEC 子状态机 `queued→drafting→drafted/blocked`；项目级不转移（SECTIONS_IN_PROGRESS 内）。

### 2.7 validate_bid_document —— 校验（确定性规则注册表 + 模型复核）
**语义**：对全部（或指定）章节执行校验：先跑**确定性规则注册表**（每规则一个 rule_id，全部可单测），再跑模型语义复核（逐节、有界上下文）。产出 FIND；**语义**：规则不过 ≠ 模型兜底——规则失败直接 blocking（"代码能查的绝不给模型"）。

确定性规则注册表（首版 ≥10 条，对应十问 Q7 的空白项）：
`AMOUNT_CONSISTENCY`(报价=Σ分项)、`QANTITY_MATCH`(数量与清单一致)、`CERT_VALIDITY`(资质有效期≥开标日)、`DATE_LOGIC`(工期/交货期算术)、`SIGNATURE_PAGE`(签章页存在)、`BLIND_BID_STRIP`(暗标标识剥离核验)、`PLACEHOLDER_FORBIDDEN`(占位符禁令)、`WORD_COUNT`(字数上下限)、`TOC_CONSISTENCY`(目录与正文一致)、`REJECTION_COVERAGE`(否决项逐条有响应)、`FORMAT_COMPLIANCE`(格式要求抽检)。

| 参数 | 类型 | 必填 |
|------|------|------|
| scope | enum | all / section_ids[] |
| ruleset | enum | core(默认)/extended |
| idempotency_key | string | ✓ |

**返回**：`{report_id, summary:{blocking,major,minor, rules_executed, rules_skipped}, findings:[{finding_id, severity, rule_id|checker:"llm_review", message, refs:{requirement_id?,section_id?,evidence_id?}}]}`
**守卫**：全部 blocking FIND 关闭（resolved/waived）后才可 →VALIDATED；report 与快照 CAS 集绑定。
**错误**：`INVALID_STATE`／`RULESET_UNKNOWN`／`KERNEL_INCOMPATIBLE`（PERMANENT：先修适配器）
**状态**：`SECTIONS_DRAFTED→VALIDATED`（守卫通过）；否则停留并返回报告。

### 2.8 export_bid_document —— 最终导出
**语义**：经文档内核适配器导出。前置：READY_FOR_EXPORT（VALIDATED + pre_export_confirm）。`meta.bid_mode=blind` 的项目**必须** `blind_mode:true`（否则报错——暗标失误是废标级事故）。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| format | enum | ✓ | docx/pdf |
| template_id | string | ✓ | 公文/技术标等模板（模板由内核模板库管理） |
| blind_mode | bool | | meta.bid_mode=blind 时强制 true |
| idempotency_key | string | ✓ | |

**返回**：`{export_job_id, artifact_ref, checksum, precheck_snapshot:{cas_set_hash, blocking_finding_count:0}}`
**错误**：`INVALID_STATE`／`BLOCKING_FINDINGS_OPEN`（NEEDS_INPUT：附 finding 清单）／`SNAPSHOT_STALE`（NEEDS_INPUT：门禁后有变更，重新走 confirm+validate）／`BLIND_MODE_REQUIRED`（NEEDS_INPUT）／`TEMPLATE_UNKNOWN`（PERMANENT）
**状态**：`READY_FOR_EXPORT→EXPORTING→EXPORTED`；产物 checksum 与快照绑定。

## 3. 辅助工具契约

### 3.1 get_bid_project_state —— 状态查询与恢复入口（Skill 恢复协议的基石）
参数：`{project_id}`。返回：`{status, stage_checklist:[{stage,done}], blocked:[{reason,ref}], open_findings:{blocking,major,minor}, sections_progress:{drafted/total}, allowed_next_tools:[...], last_event_seq, model_policy_version}`。
设计要点：`allowed_next_tools` 显式列出当前合法工具——**低能力 Agent 无需理解状态机也能走对流程**；恢复时 Skill 第一步必调此工具（见 bid-skill-design §恢复协议）。

### 3.2 attach_evidence —— 证据挂接（人/Agent 均可）
参数：`{project_id, evidence_id, target:{requirement_id?|section_id?}, quote, note?}`。
守卫：`quote` 必须能在证据内容中命中（`QUOTE_VERIFICATION_FAILED`，防引文编造）。幂等：(evidence_id, target, quote_hash)。写 EVL + 事件；不转移状态。

### 3.3 record_review_decision —— 人审门决策（含豁免）
参数：`{project_id, gate_id∈{matrix_approval,outline_approval,pre_export_confirm}, decision∈{approve,reject,waive}, target_id?(waiver 必填：requirement_id), reason(≥10 字), operator, expires_at?(waiver)}`。
守卫：门开放才可决策（`GATE_NOT_OPEN`）；waiver 过期自动失效并重新 blocking（domain-model §5）。决策记录不可变审计。

### 3.4 create_revision_task —— 发现→修订任务
参数：`{project_id, finding_ids[], section_id, instruction}`。
守卫：finding 处于 open；章节非 final（`SECTION_ALREADY_FINAL`→NEEDS_INPUT：先解除 final）。效果：REV 入队 + SEC→revision；由 draft_response_section(mode=revise) 消费。

## 4. 错误语义与重试分类

每个错误码归入三类重试语义，Agent（或框架）据此决定行为，禁止盲目重试（risk-register S-7 教训：LLM 失败静默降级）：

| 类别 | 语义 | Agent 行为 |
|------|------|-----------|
| SAFE_RETRY | 瞬态故障（超时/限流/过载） | 服务端已做指数退避；可原参重试 ≤3 次 |
| NEEDS_INPUT | 输入/决策缺陷 | 必须改变输入、补充材料或人工决策；原参重试必失败 |
| PERMANENT | 不可恢复（参数/配置/兼容性） | 放弃该路径，报告用户 |

| 错误码 | 语义 | 类别 |
|--------|------|------|
| INVALID_STATE | 状态机前置不满足（附 current_status + allowed_next_tools） | NEEDS_INPUT |
| GATE_NOT_OPEN / GATE_ALREADY_DECIDED | 人审门未开放/已决策 | NEEDS_INPUT |
| SNAPSHOT_STALE | 门禁后状态漂移 | NEEDS_INPUT |
| PROJECT/SECTION/EVIDENCE/REQUIREMENT_NOT_FOUND | 引用不存在 | PERMANENT |
| UNSUPPORTED_FORMAT / FILE_HASH_MISMATCH / TEMPLATE_UNKNOWN / RULESET_UNKNOWN | 输入不合法 | PERMANENT |
| PARSE_INTEGRITY_FAILED / COVERAGE_CHECK_FAILED / PARSE_INCOMPLETE / NEEDS_OCR | 解析/抽取不完整 | NEEDS_INPUT |
| EVIDENCE_MISSING / EVIDENCE_EXPIRED / EVIDENCE_CONFLICT / QUALIFICATION_UNMET | 证据链不满足（阻断生成/导出） | NEEDS_INPUT |
| CITATION_MISMATCH | 生成后引用回查失败 | SAFE_RETRY×1 → NEEDS_INPUT |
| QUOTE_VERIFICATION_FAILED | attach_evidence 引文编造嫌疑 | NEEDS_INPUT |
| BLOCKING_FINDINGS_OPEN | 存在未关闭的阻塞发现 | NEEDS_INPUT |
| BLIND_MODE_REQUIRED | 暗标项目未启用暗标导出 | NEEDS_INPUT |
| ATTEMPTS_EXCEEDED / VERSION_CONFLICT / SECTION_ALREADY_FINAL | 任务语义冲突 | NEEDS_INPUT |
| MODEL_TIMEOUT / MODEL_OVERLOAD / RATE_LIMITED | 模型侧瞬态 | SAFE_RETRY |
| KERNEL_INCOMPATIBLE | 文档内核版本不兼容 | PERMANENT |
| INTERNAL | 未分类内部错误 | SAFE_RETRY×1 → 上报 |

约定：错误码即稳定契约（不可删除、不可改语义，只可新增）；`agent_hint` 一句话自救指引必填。

## 5. 状态转换矩阵（工具 × 状态）

| 当前状态 | 允许的变更工具 | 转换结果 |
|---------|---------------|---------|
| — | create_bid_project | CREATED |
| CREATED | import_tender_files | FILES_IMPORTING→PARSED/PARSE_FAILED |
| PARSED / PARSE_FAILED(修复后) | import_tender_files(重入) / extract_requirements | →REQUIREMENTS_READY |
| REQUIREMENTS_READY | import_tender_files(仅 addendum/clarification) / get_compliance_matrix / attach_evidence / record_review_decision(matrix_approval) | →MATRIX_APPROVED |
| MATRIX_APPROVED | （目录生成由 extract 后的编排内部完成）record_review_decision(outline_approval) | →OUTLINE_APPROVED |
| OUTLINE_APPROVED | draft_response_section / create_revision_task / get_bid_project_state | →SECTIONS_IN_PROGRESS→SECTIONS_DRAFTED |
| SECTIONS_DRAFTED | validate_bid_document / create_revision_task | →VALIDATED（无 blocking） |
| VALIDATED | record_review_decision(pre_export_confirm) | →READY_FOR_EXPORT |
| READY_FOR_EXPORT | export_bid_document | →EXPORTING→EXPORTED |
| 任意 | get_bid_project_state / search_evidence / get_compliance_matrix(≥REQ_READY) | 纯读 |
| EXPORTED | get_bid_project_state；失效传播仅告警 | 产物已交付不回滚 |

> 目录生成说明：OUTLINE 由"format 条款规则推导 + 招标规定目录优先 + 模型装配"的服务端内部流程产出（吸收 ai_bidding 确定性格式链），不单独开工具——减少工具面；Agent 可在 OUTLINE_DRAFTED 前经 get_compliance_matrix 与人交互调整后走门。

## 6. 设计说明与反例（审计对照）

| 设计决策 | 审计证据 |
|---------|---------|
| 业务级工具面（12 个），不暴露 SQL/文件细粒度 | tender-cli MCP 仅示例工具=零业务价值；OpenBidKit 内核"单行 JSON+任务目录"证明粗粒度业务协议可长期演进 |
| 服务端组上下文包（五域上下文：要求/证据/大纲/风格/字数） | 15 仓库普遍把全文塞模型（12k~400k 截断根源）；ai_bidding v2 的 context_builder 是唯一正解样本 |
| `allowed_next_tools` 随 INVALID_STATE 返回 | 低能力模型误调是 tender-cli/Get00 断链的共同根因（S-6） |
| 完整性对账内置于 import/extract | S-1：15 仓库无一有对账 |
| 证据默认过滤过期（include_expired 开关） | ark 唯一做过期过滤 |
| 生成后引用回查（CITATION_MISMATCH） | ai_bidding CIT 只登记输入侧；无一仓库做输出侧回查 |
| 暗标强制校验（BLIND_MODE_REQUIRED） | OpenBidKit BlindCheck 是唯一暗标实现，其余全缺 |
| 异步作业+轮询（import/extract/export） | 长任务同步等待导致 llm_bidwriting 永久挂起（R-25） |
| 一切决策留痕（decided_by/model_run_id/operator） | 15 仓库的模型判断均不可审计 |

## 7. 并发与安全

- **项目级**：同一项目的阶段转换串行（advisory lock：stage-transition）；**章节级**：同一项目内 SectionTask 可并行（per-section 锁），但 validate/export 期间全项目冻结变更（write lock）。
- **幂等键**：24h 窗口，键唯一约束在事件存储层（与工具调用 causation_id 关联）。
- **授权**：MCP 会话绑定操作者身份；record_review_decision 必须为 human 角色（Agent 不可代人审门——人审门的意义所在）；waiver 需 reason + expires_at 双件套。
- **速率**：模型调用全走服务端配额（model_run 记录 token/成本），Agent 无法绕过成本门。

