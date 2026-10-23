# AI 标书业务系统 · 开源调研 + 源码审计 + Skill & MCP 架构设计

> 阶段产出（研究/审计/设计，不实现）：审计 **15 个开源标书仓库**（用户指定 5 + GitHub 搜索筛选 10，共去重候选 92 个），基于源码证据形成 Skill + MCP + 后端状态机的目标架构设计与 MVP 实施计划。
> 审计方式：只读静态源码审计，每个结论附文件+函数级证据；README 声明一律不作为实现证据。

## 结论速览（3 分钟版）

1. **生态空白**：15 个深度审计仓库中，没有任何一个同时具备「Skill + 业务级 MCP 工具 + 状态化后端」。真 Dify DSL 全网仅 1 家（且为最弱实现），真 MCP 2 家（示例级/静默 stub），真 Skill 5 家。**目标架构没有现成可搬的组合，业务 MCP 层必须自建。**
2. **两大系统性缺陷**：① 截断丢条款（12k~400k 各种静默截断，无一家有完整性校验）；② 硬阻断可绕过（门禁在前端/API 不校验/纯提示词自律）。
3. **架构决策**：采用**路径 A（后端固定状态机 + 12 个业务级 MCP 工具 + 2 个 Skill）**；**不采用 Dify、不采用 LangGraph**（均有审计反例：唯一 Dify 用户实现最弱；唯一 LangGraph 用户移除了人审门、图退化两节点、恢复默认关闭）。
4. **复用底牌**：代码级复用首选 `NewbieCoderLab/ai_bidding` 与 `sunShineLoveMe/ai_bidding_power_grid`（MIT、全链、带测试）；`96syh/biaoshu`（MIT，需剔除 AGPL 的 PyMuPDF）；`Get00/BiaoShu-SKILL` 工具层与 `bidcraft`（Apache）。AGPL×3、无许可证×5 仅可 clean-room 借鉴；**3 家许可证虚标（README 与 LICENSE 文件矛盾）、2 家泄露真实企业数据**——复用纪律见 `research/repository-matrix.md` §6。
5. **一句话流程**：导入（完整性对账）→ 六类条款抽取（覆盖率对账，否决项 100% 召回硬门）→ 证据三通道检索 → 合规矩阵 + 三道人审门 → 规则推导目录 → 单章任务化生成（引用回查）→ 规则注册表审校（确定性优先）→ 内核导出（暗标强制校验），全程事件溯源可断点恢复。

## 文档地图

| 文档 | 内容 |
|------|------|
| `research/repository-matrix.md` | 15+9 仓库对比矩阵、能力矩阵（a–p×16）、复用分级、纳入/排除理由、许可证风险总览、横向结论 |
| `research/workflow-audit.md` | 每仓库真实执行链路（入口/数据结构/持久化/失败处理/缺失节点）+ **十问综合回答** + 缺口→架构要求映射 |
| `research/risk-register.md` | 41 条仓库风险台账（证据+复现方法）、8 条共性失败模式、10 条本项目前推风险与缓解 |
| `research/test-plan.md` | 合成招标语料设计（120 条种子条款/needle 证据/12 类缺陷种子）+ 10 组用例与验收阈值 |
| `research/audit-notes/` | 15 份逐仓库深度审计 + 2 份初筛报告（全部源码证据的原始出处） |
| `docs/bid-domain-model.md` | 领域实体（REQ/EV/CMP/SEC/FIND/REV…）、项目状态机、失效传播矩阵、**文档内核 8 操作最小契约** |
| `docs/bid-mcp-contract.md` | 12 个 MCP 工具契约（参数/返回/守卫/幂等/状态转换）+ 错误码三级重试语义 |
| `docs/bid-skill-design.md` | 主 Skill `bid-authoring` 全文草案（铁律/6 阶段/停止条件/恢复协议）+ 审校子 Skill + 工程规范 |
| `docs/architecture-decision.md` | 路径 A/B 五维对比、Dify/LangGraph 裁定与证据、实践投票、反例与重评触发、7 条 ADR |
| `docs/mvp-implementation-plan.md` | M0–M5 实施顺序、每步验收标准、里程碑×风险对照、复用执行纪律 |

## 建议阅读顺序

1. 本 README → `research/repository-matrix.md` §7（横向结论）
2. `research/workflow-audit.md` §2（十问——直接回答招标长文件/条款/证据/阻断/恢复等核心问题）
3. `docs/architecture-decision.md`（为什么不是 Dify/LangGraph）
4. `docs/bid-domain-model.md` → `docs/bid-mcp-contract.md` → `docs/bid-skill-design.md`（设计三件套）
5. 动手前：`docs/mvp-implementation-plan.md` + `research/test-plan.md`；证据溯源随时查 `research/audit-notes/`

## 证据等级与纪律

- 每份审计报告记录：文件路径、函数/类名、关键代码引用；标注 [复核] 的结论经主代理二次验证（Dify DSL 可解析性、SKILL.md frontmatter、必崩 bug、死码、许可证虚标、AGPL 切换史、依赖陷阱等 10 项全部复核通过）。
- 能力标签口径：IMPLEMENTED / PARTIAL / SPEC_ONLY / ROADMAP / UNVERIFIED / MISSING；README 声明不算实现证据。
- **复用红线**：许可证以 LICENSE 文件为准；AGPL/无许可证仓库仅可 clean-room 借鉴；任何来源的真实招标数据/企业内部规则不得进入本系统（详见 `repository-matrix.md` §6）。

## 下一步（按 mvp-implementation-plan.md）

M0 地基（状态机+契约+stub 内核，全量无 LLM 测试）→ M1 解析与条款（否决项召回 100% 硬门）→ M2 证据与矩阵 → M3 章节生成（断点恢复）→ M4 审校（确定性规则 100%）→ M5 导出与端到端（暗标零泄漏）。文档内核在 M0 以 stub 契约冻结接口、M5 才接真实实现，全程不修改内核内部。
