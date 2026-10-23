# 标书主 Skill 设计（bid-skill-design）

> 定位：Skill 是给模型的**可重复执行流程知识**；服务端是**权威状态机**；模型只做局部判断与文本生成。
> 设计依据：4 个真实 Skill 仓库的正反经验——ark（校验器+evals 最完整）、Get00（文档与脚本失配 ≥10 处 + 流水线必崩 = 反面教材）、kcaaaa（门禁/熔断/断点机制设计最全但零代码强制）、LingYan（6 个 Skill 绑定 deepagents 运行时）。
> 核心结论：**Skill 只引用 MCP 工具名与语义，绝不假设脚本存在、绝不硬编码 CLI 参数**（Get00 病，risk-register P-9）。

## 1. Skill 体系总览

| Skill | 形态 | 触发 | 职责 |
|-------|------|------|------|
| `bid-authoring`（主） | SKILL.md，约 200 行 | 编制标书/响应招标/补遗更新/续跑项目 | 6 阶段主流程、停止条件、恢复协议 |
| `bid-review`（辅） | SKILL.md，约 80 行 | validate 后处理 findings、人工复核辅助 | 逐 finding 生成修订指令，喂给 create_revision_task |

刻意只做 **2 个** Skill：审计显示 13 个子 Skill（kcaaaa）在实际执行中无代码强制、形同文档；流程约束应尽量下沉到服务端状态机与工具守卫，Skill 只保留"模型必须知道的操作顺序与纪律"。

## 2. 主 Skill 草案：`bid-authoring`（可直接使用的全文）

````markdown
---
name: bid-authoring
description: >
  使用场景：用户要求编制/撰写/生成投标文件（标书）、响应招标文件、在补遗或澄清后
  更新标书、或继续一个未完成的标书项目。本 Skill 通过 bid-mcp 的 12 个工具、在服务端
  状态机约束下完成全流程。开始任何写作之前，必须先调用 get_bid_project_state。
  禁止在本 Skill 之外手工编写标书正文。
---

# 标书编制主流程

## 铁律（违反任何一条，立即停止并向用户报告）
1. **证据先行**：任何事实断言（资质、业绩、金额、日期、型号、人员）必须来自
   search_evidence 返回的证据，且该证据必须能挂接（attach_evidence）到对应要求。
   搜不到证据 = 不写这句话。禁止编造、禁止用"行业通常做法"顶替事实。
2. **状态机权威**：进展只能通过工具推进。任何工具返回 INVALID_STATE 时，按返回的
   allowed_next_tools 修正下一步；绝不绕过，绝不在对话里"假装完成"。
3. **确定性检查优先**：金额、数量、日期、有效期、签章、暗标等数值与格式判断，
   一律交给 validate_bid_document 的规则注册表；不要自己心算或目测。
4. **引用用 ID**：复述招标条款只用 REQ-ID + 不超过 40 字的摘录；不粘贴大段原文。
5. **阶段推进**：每个阶段只有满足"停止条件表"中该阶段的 PASS 条件才可进入下一阶段。

## 启动与恢复协议（每次会话的第一步，无条件执行）
1. 调用 get_bid_project_state（有多个项目时先询问用户项目名）。
2. status=CREATED → 从阶段 1 开始。
3. 项目有进展 → 从 stage_checklist 第一个 done=false 的阶段继续。
   **禁止重做已完成阶段**；服务端会拒绝重复工作，被拒绝就回到本协议第 1 步。
4. blocked 非空 → 先向用户逐条报告阻断原因与涉及的 REQ/EV-ID，等待人工决策；
   等待期间不得生成任何正文。

## 阶段流程（每阶段 ≤5 次工具调用后必须做一次状态核对）

### 阶段 1 文件导入
create_bid_project（external_ref 去重）→ import_tender_files
（main 文件在前；addendum/clarification 必须按用户指明的角色导入）
→ 轮询 get_bid_project_state 至 PARSED 或失败。
- STOP：PARSE_INTEGRITY_FAILED / NEEDS_OCR → 报告用户等待修复，不进入阶段 2。
- PASS：全部文件 parse_status=parsed。

### 阶段 2 条款抽取
extract_requirements → 检查返回的 summary 与 quality_flags。
- STOP：coverage 不通过 / COVERAGE_CHECK_FAILED → 报告未覆盖块清单。
- STOP：quality_flags 含 NO_REJECTION_CLAUSE（抽取结果为 0 条否决项）→ 请用户确认
  "该招标文件是否确实没有否决条款"；仅在用户确认后继续（决策由
  record_review_decision 承担，不要口头默认）。
- PASS：进入 REQUIREMENTS_READY 且无未处理 quality_flags。

### 阶段 3 合规矩阵与人审
get_compliance_matrix（分页拉全量，blocked_only=false）→ 向用户呈现三类清单：
已满足 / 部分满足 / 缺证据或不满足（附 REQ-ID、EV-ID、missing 原因）
→ 引导用户对不满足项做出决策（补证据 / waiver），用户通过 record_review_decision
（gate=matrix_approval）放行。
- STOP：blocking_count>0 且用户未完成决策 → 停止，等待。
- PASS：MATRIX_APPROVED。

### 阶段 4 目录确认与章节生成
提示用户确认目录（gate=outline_approval）→ 对每个 queued 章节调用
draft_response_section，逐章汇报（成功/被阻断/重试）。
- 单章收到 EVIDENCE_MISSING → 该章标记 blocked，**继续其他章节**，不空等；
  全部章节处理完后汇总 blocked 清单给用户。
- 重试纪律：SAFE_RETRY 类错误重试 ≤3 次；NEEDS_INPUT 类错误不重试原参数，
  改为报告或换输入。
- STOP：单章 ATTEMPTS_EXCEEDED → 该章转人工，其余章节照常。
- PASS：SECTIONS_DRAFTED（或完成全部可执行章节并已汇报 blocked）。

### 阶段 5 审校与修订
validate_bid_document → 向用户呈现 findings 摘要（blocking/major/minor 分列）
→ 对每个 blocking/major finding 走 bid-review 子技能产出修订指令
→ create_revision_task → draft_response_section(mode=revise) 循环
→ 再次 validate_bid_document。
- STOP：修订两轮后 blocking 仍不清零 → 停止并向用户请求人工介入。
- PASS：VALIDATED（blocking=0）。

### 阶段 6 导出
向用户确认 record_review_decision(pre_export_confirm)
→ export_bid_document（format 与 template 由用户选择；
  **meta.bid_mode=blind 的项目必须 blind_mode=true**，否则调用会被拒绝）
→ 交付 artifact_ref 与 checksum，并提示"导出后再修改章节会使快照失配"。

## 补遗 / 澄清处理（增量，不全量重跑）
import_tender_files(role=addendum) → extract_requirements（增量范围）
→ get_compliance_matrix 查受影响章节 → 仅对被标记 revision 的章节执行
draft_response_section(mode=revise) → 重新走阶段 5、6。
**绝不允许**因为补遗而重做未受影响章节。

## 失败与降级纪律
- 工具错误按 retry_class 行动：SAFE_RETRY ≤3 次（间隔递增）；NEEDS_INPUT 报告用户；
  PERMANENT 停止该路径。
- 连续 3 个不同章节返回同类 NEEDS_INPUT 错误 → 视为系统性问题，全阶段暂停并报告。
- 严禁在错误后"绕过工具直接在聊天里输出正文"。

## 汇报格式（每次向用户汇报）
1. 结论一句话（处于哪个阶段、能否继续）；
2. 三栏清单：已完成（含 REQ/SEC-ID）/ 进行中 / 被阻断（含错误码与所需决策）；
3. 下一步建议（allowed_next_tools 的白话翻译）。
严禁在工具返回成功之前声称"已完成"。
````

## 3. 停止条件总表（服务端与 Skill 双重强制）

| 阶段 | STOP 条件 | 谁强制 | 恢复动作 |
|------|----------|--------|---------|
| 导入 | 解析完整性对账失败 / 需 OCR | 服务端（PARSE_INTEGRITY_FAILED） | 修复文件或走 OCR 后重导入 |
| 抽取 | 覆盖率不达标 / 零否决项未确认 | 服务端 + Skill | 修 chunk / 人确认后继续 |
| 矩阵 | blocking 项未决策 | 服务端（GATE_NOT_OPEN） | record_review_decision |
| 生成 | 单章证据缺失 / 次数耗尽 | 服务端（EVIDENCE_MISSING / ATTEMPTS_EXCEEDED）+ Skill（跳过策略） | 补证据或人工 |
| 审校 | 两轮修订 blocking 不清零 | Skill（熔断） | 人工介入 |
| 导出 | 快照失配 / 暗标未启用 | 服务端（SNAPSHOT_STALE / BLIND_MODE_REQUIRED） | 重新确认+validate |
| 任意 | 连续 3 章同类 NEEDS_INPUT | Skill（系统性熔断，kcaaaa 三重熔断的代码化） | 暂停并报告 |

## 4. 恢复机制（与事件溯源配合）

1. **入口**：恢复的唯一入口是 get_bid_project_state（Skill 铁律 §启动协议）。
2. **状态定位**：stage_checklist + sections_progress 给出精确断点；Agent 不需要记忆。
3. **不重做**：已完成阶段的工具调用会被幂等/状态守卫拒绝；Skill 明确"被拒绝→重新读状态"，与 bidcraft 默认关闭的 auto_resume 相反，**恢复默认开启**（risk-register R-20 反例）。
4. **失效感知**：恢复时若 blocked 非空（例如恢复前证据刚过期），Skill 必须先报告再行动——这正是 15 仓库全部缺失的"恢复后世界可能已变"（十问 Q8）。
5. **换模型**：model_policy_version 变更不自动失效已起草章节；Skill 在恢复协议中提示用户"建议重新 validate"。

## 5. 审校子 Skill：`bid-review`

````markdown
---
name: bid-review
description: >
  使用场景：validate_bid_document 返回 findings 之后，需要逐条分析 finding 并生成
  修订指令时使用。只产出指令文本与 create_revision_task 调用，不直接改正文。
---
# 逐 finding 处理流程
对每个 blocking/major finding：
1. 读取 finding.refs 指向的 requirement（get_compliance_matrix 过滤该 REQ-ID）
   与涉及章节的 evidence 清单。
2. 判断类别：
   a. 事实类（金额/日期/数量/有效期）→ 不自行判断对错，指令 = "按证据 EV-xxx 修正"
      （证据冲突则转人工，不要选边）。
   b. 覆盖类（否决项无响应）→ 指令 = "补充响应，要求编号 REQ-xxx"。
   c. 表述类（占位符/格式/字数）→ 指令 = 具体改写要求。
   d. 暗标类（BLIND_BID_STRIP）→ 指令 = "剥离以下标识信息后重写该段"。
3. 生成 ≤80 字修订指令 → create_revision_task（finding_ids, section_id, instruction）。
4. 一批（≤5 条）处理完后向用户复述指令清单，再统一执行修订。
禁止：为 minor finding 创建修订任务（避免 token 浪费，minor 汇总给人审）。
````

## 6. Skill ↔ MCP 映射（每个阶段的合法工具）

| 阶段 | 工具 | 只读/变更 |
|------|------|----------|
| 启动/恢复 | get_bid_project_state | 只读（每次会话必调） |
| 1 导入 | create_bid_project, import_tender_files | 变更 |
| 2 抽取 | extract_requirements | 变更 |
| 3 矩阵 | get_compliance_matrix, search_evidence, attach_evidence, record_review_decision | 读+挂接+人审 |
| 4 生成 | draft_response_section（revise 消费修订任务） | 变更 |
| 5 审校 | validate_bid_document, create_revision_task（+bid-review 子技能） | 变更 |
| 6 导出 | export_bid_document（+pre_export_confirm 人审） | 变更 |
| 全程禁用 | —— 任何未在 mcp-contract §1 总表中的操作 | — |

## 7. 工程规范与验收

1. **版本化**：SKILL.md 纳入 git，语义变更升版本号；eval 集随版本走（ark 的 evals 模式）。
2. **描述即触发器**：frontmatter description 写"使用场景"，不写功能清单（kcaaaa 的 12 子 Skill 暴露的检索稀释问题）。
3. **无脚本假设**：Skill 全文只出现 12 个工具名，参数以 MCP server 下发的 schema 为准；CI 校验"Skill 中出现的工具名 ⊆ 契约总表"（直接防 Get00 病）。
4. **可演练**（dry-run）：以 stub 内核 + Mock LLM 走全流程，断言 Skill 的工具调用序列合法（见 test-plan T10）。
5. **低能力模型适配**：每步指令为短句祈使句；每阶段 ≤5 次工具调用强制一次状态核对；错误自救只依赖 agent_hint 与 allowed_next_tools，不要求模型理解状态机。
6. **验收标准**：合成语料（test-plan §2）上，一个从未见过该项目的会话仅凭 Skill 从 EXPORTED 断点恢复并完成导出，全程 0 次非法工具调用、0 次编造断言（引用回查全通过）。
