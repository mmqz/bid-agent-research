# 开源标书仓库对比矩阵（repository-matrix）

> 调研日期：2026-10-23 · 方式：只读静态源码审计（未运行任何仓库代码）
> 证据等级：**[深审]** 逐文件源码审计（报告见 `research/audit-notes/<仓库>.md`）；**[复核]** 主代理对关键结论二次验证；**[初筛]** 快速事实核查（`screening-a.md` / `screening-b.md`）
> 许可证口径：一律以仓库 LICENSE 文件为准；README 徽章/manifest 声明不作为许可证证据（本次发现 3 处虚标，见 §6）。

## 0. 方法与口径

- 搜索来源：GitHub Repository Search（12 组中英文查询）+ Code Search（`SKILL.md`、Dify yml 特征、解析代码特征），合并去重 **92 个候选仓库**。
- 深度审计对象：**15 个**（用户指定 5 + 初筛入选 10），每个仓库独立审计，能力标签为 IMPLEMENTED / PARTIAL / SPEC_ONLY / ROADMAP / UNVERIFIED / MISSING。
- 关键负面/正面结论均经主代理抽查复核：Dify DSL 可解析性、SKILL.md frontmatter、`Get00` 流水线必崩 bug、`ai_bidding` 响应矩阵死码、`BidMaster-Pro` 许可证虚标、`LingYan` AGPL→MIT 切换史、`tender-cli` MCP 依赖陷阱、`bidcraft` interrupt 移除、`power_grid` 关键词式废标识别——全部复核通过（复核脚本 `scripts/spotcheck.py` 思路记录于 worklog）。

## 1. 深度审计仓库总表

| # | 仓库 | 星标 | 许可证(实际) | 最近推送 | 主语言 | 执行链路判定 | 复用判定 |
|---|------|-----|-------------|---------|--------|-------------|---------|
| 1 | [anlaiye/ark-ai-bid-agent](audit-notes/anlaiye__ark-ai-bid-agent.md) | 1 | **无** | 2026-08-04 | TypeScript | 完整链（工作台+本地RAG服务+Codex Skill） | 仅借鉴（无许可证） |
| 2 | [96syh/biaoshu](audit-notes/96syh__biaoshu.md) | 0 | MIT | 2026-06-19 | Python | 完整链 8/8（62+10 测试） | 需改造复用（剔除 PyMuPDF） |
| 3 | [joychin/LingYan-Bid-Agent](audit-notes/joychin__LingYan-Bid-Agent.md) | 3 | MIT（现，经 AGPL 窗口） | 2026-09-29 | Python | 完整链（重工程，2.1万行测试） | 可直接复用（限 HEAD） |
| 4 | [Chengyihaowen/llm_bidwriting](audit-notes/Chengyihaowen__llm_bidwriting.md) | 0 | **无** | 2026-04-06 | Vue+Flask | 部分链（**全调研唯一真 Dify DSL×3**） | 仅借鉴 + DSL 结构参考 |
| 5 | [guangshu100/BidMaster-Pro](audit-notes/guangshu100__BidMaster-Pro.md) | 406 | **AGPL-3.0**（README 虚标 MIT）[复核] | 2026-09-03 | Python | 完整链（演示级，零测试） | 不建议采用 |
| 6 | [NewbieCoderLab/ai_bidding](audit-notes/NewbieCoderLab__ai_bidding.md) | 62 | MIT | 2026-06-24 | Python | v2 分册链完整；矩阵为死码 [复核] | **可直接复用（首选）** |
| 7 | [FB208/OpenBidKit_Yibiao](audit-notes/FB208__OpenBidKit_Yibiao.md) | 3202 | AGPL-3.0 | 2026-10-22 | JS/Electron | 主链完整 + 8 相位恢复 | 仅借鉴（AGPL） |
| 8 | [Get00/BiaoShu-SKILL](audit-notes/Get00__BiaoShu-SKILL.md) | 186 | Apache-2.0 | 2026-09-24 | Python(Skill) | 部分链（**主流水线第3步必崩** [复核]） | 需改造复用（工具层+SKILL骨架） |
| 9 | [Router0824/BidPilot-AI](audit-notes/Router0824__BidPilot-AI.md) | 102 | **AGPL-3.0**（徽章误标 Apache） | 2026-10-03 | Python | 部分链（11节点状态机真实） | 仅借鉴（AGPL） |
| 10 | [sunShineLoveMe/ai_bidding_power_grid](audit-notes/sunShineLoveMe__ai_bidding_power_grid.md) | 5 | MIT | 2026-10-22 | Python | 完整链 9 环（16 测试） | 可直接复用 |
| 11 | [zeronezer/bidcraft](audit-notes/zeronezer__bidcraft.md) | 73 | Apache-2.0 | 2026-10-09 | Python | 部分链（审校、导出缺失） | 需改造复用 |
| 12 | [aigc-open/tender-cli](audit-notes/aigc-open__tender-cli.md) | 48 | Apache-2.0（pyproject 误标 MIT） | 2025-09-13（停滞1年） | Python | 部分链（多处断裂；真 FastMCP） | 需改造复用（MCP 范式） |
| 13 | [kcaaaa/bid-writing-agent](audit-notes/kcaaaa__bid-writing-agent.md) | 3 | **无** | 2026-08-12 | 纯提示词 | 全链 SPEC_ONLY（13 个 SKILL.md） | 仅借鉴（机制设计） |
| 14 | [mrwisdom2/biaoshuv2](audit-notes/mrwisdom2__biaoshuv2.md) | 1 | **无 + 含真实企业数据** | 2026-06-23 | Python | 审校链真实（无生成链） | 不建议采用（法律红线） |
| 15 | [RudyJunyu/Bid_Agent](audit-notes/RudyJunyu__Bid_Agent.md) | 38 | **无 + 数据泄露** | 2026-06-14 | Python | 部分链（解析最强，后半弱） | 仅借鉴（clean-room） |

## 2. 能力矩阵

标签缩写：**I**=IMPLEMENTED，**P**=PARTIAL，**S**=SPEC_ONLY，**U**=UNVERIFIED，**M**=MISSING。能力项定义见各审计报告 §2。

### 2.1 业务能力（a–h：解析 → 导出）

| # | 仓库 | a 解析 | b 六类条款 | c 溯源 | d 证据检索 | e 响应矩阵 | f 目录分章 | g 审校 | h 导出 |
|---|------|-------|-----------|-------|-----------|-----------|-----------|--------|--------|
| 1 | ark-ai-bid-agent | I | I | P | I | I | P | P | I |
| 2 | 96syh/biaoshu | I | I | P | P | I | I | P | I |
| 3 | LingYan-Bid-Agent | I | I | **I** | P | P | I | P | P |
| 4 | llm_bidwriting | P | P | M | P | M | I | P | I |
| 5 | BidMaster-Pro | I | P | M | P | P | I | P | I |
| 6 | ai_bidding | P | P | P | **I** | S(死码) | I | P | I |
| 7 | OpenBidKit_Yibiao | I | P | P | P | M | I | I | I |
| 8 | BiaoShu-SKILL | P | P | M | M | M | P | P | P |
| 9 | BidPilot-AI | P | P | P | P | P | I | P | I |
| 10 | ai_bidding_power_grid | I | I | I | P | I | I | P | I |
| 11 | bidcraft | I | P | P | P | P | I | **M** | **M** |
| 12 | tender-cli | P | P | M | M | M | P | P | P |
| 13 | bid-writing-agent | S | S | S | M | M | S | S | S |
| 14 | biaoshuv2 | P | M | I | M | M | M | **I** | P |
| 15 | RudyJunyu/Bid_Agent | **I** | P | P | P | M | I | P | P |

### 2.2 工程能力（i–p：持久化 → LLM 调用）

| # | 仓库 | i 持久化恢复 | j 失败处理 | k 可观测 | l 测试 | m Skill | n Dify DSL | o MCP | p LLM调用 |
|---|------|-------------|-----------|---------|--------|---------|-----------|-------|----------|
| 1 | ark-ai-bid-agent | I | I | P | P | **I** | M | M | I |
| 2 | 96syh/biaoshu | I | I | P | I | P | M | P | I |
| 3 | LingYan-Bid-Agent | **I** | I | I | **I** | I | M | M | I |
| 4 | llm_bidwriting | P | P | M | M | M | **I** | M | I |
| 5 | BidMaster-Pro | P(假恢复) | I | P | M | P | M | P(stub) | I |
| 6 | ai_bidding | I | P | I | I | M | M | M | I |
| 7 | OpenBidKit_Yibiao | **I** | I | I | P | P | M | M | I |
| 8 | BiaoShu-SKILL | P | P | P | M | I | M | M | M(宿主驱动) |
| 9 | BidPilot-AI | I | I | P | I | M | M | M | I |
| 10 | ai_bidding_power_grid | I | I | I | I | M | M | M | I |
| 11 | bidcraft | I(默认关) | I | P | P | M | M | M | I |
| 12 | tender-cli | P | P | I | P | M | M | P | I |
| 13 | bid-writing-agent | S | S | S | M | **I** | M | M | U |
| 14 | biaoshuv2 | P | P | I | I | P | M | M | I |
| 15 | RudyJunyu/Bid_Agent | P | P | P | M | M | M | M | I |

## 3. 各仓库纳入理由与复用价值

### 3.1 anlaiye/ark-ai-bid-agent 【用户指定 · 仅借鉴】
- **形态**：Next.js16 工作台前端 + 本地 Fastify RAG 服务（PostgreSQL/pgvector + Ollama/bge-small-zh）+ 独立 Codex Skill，三层组合。
- **纳入理由**：唯一同时做到「LLM+正则双轨条款抽取互验」「证据过期 SQL 硬过滤」「Skill 自带 201 行 stdlib 确定性校验器 + evals」的仓库；其 `response_matrix` 逐条判定结构完整。
- **关键证据**：`parsers.js extractText`（扫描件显式拒绝）；`generator.js` 六分类双轨；PG 10 表 + 版本快照；`SKILL.md` frontmatter 已验证 [复核]。
- **不可复用原因**：无 LICENSE（默认版权保留）；导出硬阻断仅在前端（`server.js:331-347` API 不校验）；70k 字符截断 + 120 条上限。
- **可借鉴**：Skill 确定性校验契约、双轨互验、证据过期过滤、风险规则外置重算。

### 3.2 96syh/biaoshu 【用户指定 · 需改造复用】
- **形态**：Python 单体全流程（5 角色 Agent → AnalysisReport，40+ 数据模型/ID 体系），MIT。
- **纳入理由**：链路 8/8 真实（含 SQLite 落盘、python-docx + TOC 域导出、400 字人工确认门），62 后端 + 10 前端测试。
- **可直接复用（MIT）**：`schema_defs` 全套数据模型与 `utils/prompts` 提示词、`fallback_generation` 确定性抽取。
- **必须改造**：默认依赖 PyMuPDF 为 AGPL（传染性），需整体替换为 pdfium/MinerU 等宽松许可解析器。
- **风险**：>45k 长文关键词窗口裁剪可能系统性丢条款且无遗漏告警；`ready_to_export=false` 不在代码层拦截导出。

### 3.3 joychin/LingYan-Bid-Agent 【用户指定 · 可直接复用（限 HEAD）】
- **形态**：Python 重工程单体（4.6 万行代码 + 67 测试文件 2.1 万行 + 6 个 deepagents SKILL.md 全部 frontmatter 合法 [复核]）。
- **纳入理由**：全调研**溯源最强**（引用键 + 行号/页码机器校验）、**恢复最强**（app.db + agent.db checkpoint + 断点续跑 + AIMD 重派守卫）、docx 内核 3645 行（修订标记/元素级注入/合册）。
- **复用前提**：仅在当前 HEAD 上复用——git 历史显示 2026-09-10 曾设 AGPL-3.0-only、09-16 切回 MIT [复核]，严格避开 09-10~09-16 窗口的代码。
- **依赖注意**：`deepagents==0.7.7` 强版本锁定；SKILL 需连同 22 个工具一起移植。
- **风险**：抽取内容真伪无机器校验（只验引用键格式，引对行抄错内容检不出）；资格符合性无硬阻断。

### 3.4 Chengyihaowen/llm_bidwriting 【用户指定 · 仅借鉴 + DSL 参考】
- **形态**：Vue3 六模块向导前端 + Flask 七蓝图后端 + **Dify 外挂**；git 历史含真实业务数据，证明真实运行过。
- **纳入理由**：**全调研唯一存在可导入 Dify DSL 的仓库**——3 份（招标文件解析/章节生成/废标检查），YAML 全部可解析、`app.mode=workflow`、含完整图定义 [复核]；但存在变量注入 bug（废标检查把"当前标书正文"误绑 `parsed_bid`）、需配模型插件才能跑。
- **不可复用原因**：无 LICENSE；能力短板多（无溯源、无矩阵、纯 2-gram 词法检索、检查未注入正文）。
- **可借鉴**：三流拆分、`GenerationTask`+SSE 逐章生成、markdown→docx 渲染、反幻觉提示词体系、**Dify DSL 的节点/变量组织方式**（作为结构参考）。

### 3.5 guangshu100/BidMaster-Pro 【用户指定 · 不建议采用】
- **形态**：Python 全流程 Agent（README 声称 406 星口碑、21 项合规检查）。
- **纳入理由**：星标最高的用户指定仓库，必须查证其真实成色——结论：**README 许可证虚标（MIT → 实为 AGPL-3.0）** [复核：LICENSE 文件首行 "GNU AFFERO GENERAL PUBLIC LICENSE"，README 第 7/375 行标 MIT]；15 项检查为 LLM 提示词 + 32k 截断；checkpoint 只存 key 名、resume 为全量重跑；零测试 + 7 处依赖错配；FastMCP 8 工具因依赖错配静默退化为 Stub。
- **可借鉴**：15 维解读归一化、评分项→章节映射、公文排版 885 行参数、防编造占位符、错误策略表。
- **结论**：AGPL + 虚标 + 零测试，代码一行不可复用。

### 3.6 NewbieCoderLab/ai_bidding 【补充 · **可直接复用（首选）**】
- **形态**：Python + MySQL 43 表 + PGVector 混合检索 + v2 分册事件溯源；57 个测试（sys.modules 桩，零基础设施可离线跑）。
- **纳入理由**：许可证最干净（MIT）+ 工程最完整；响应矩阵写入函数 `mark_response_matrix_coverage` 全库零调用被证伪为死码 [复核]，但 `_check_scoring_coverage`（`api/bidding.py:1130`）评分级覆盖真实可用——两处证据都已在报告中标明。
- **可直接复用（MIT，具体到文件）**：①确定性格式目录链（`tender_format_parser` 等 4 文件 + 20 测试，零 LLM/DB 依赖）②PGVector 混合检索（`hybrid_search` + reranker + `001_*.sql`）③`md_to_word` 中文导出 ④`services/v2` 事件溯源 + 五域上下文 + CIT 证据登记。
- **风险**：12k 预分析截断 + 无表格/OCR 解析 → 长招标条款静默丢失；`requireEvidence` 策略仅存库无代码消费；citation 只登记喂入证据、无生成后引用回查。

### 3.7 FB208/OpenBidKit_Yibiao 【补充 · 仅借鉴（AGPL）】
- **形态**：Electron 桌面应用（3202 星、极活跃），SQLite 50+ 表、8 相位恢复、C# OpenXML 文档内核。
- **纳入理由**：星标最高、审校工程最强（64 条 C# 确定性规则 + 查重四维算法 + 暗标 BlindCheck 全家桶 + 3k 行暗标 E2E）；其 C# 文档内核「单行 JSON 信号 + 任务目录」协议与我们"文档内核独立"的目标架构**同构**，协议形态可直接平移。
- **不可复用原因**：AGPL-3.0（含 SaaS §13 网络分发义务）；通用代码硬编码客户串（`duplicateCheckService.cjs:1893`）。
- **可借鉴**：内核通信协议、废标三轮 + 滚动补丁、规则/候选分工、查重算法、8 相位恢复。

### 3.8 Get00/BiaoShu-SKILL 【补充 · 需改造复用】
- **形态**：纯 Skill 包（805 行 `SKILL.md` 真实存在 [复核] + 14 步流水线脚本），Apache-2.0。
- **纳入理由**：搜索命中的最典型「Skill 形态」标书仓库；行业自动检测 + 评分标准解析的 SKILL 结构有直接参考价值。
- **关键事实**：**主流水线第 3 步必崩**——`bid_writer_pipeline.py:473` 调 `extractor.extract()`，而 `RequirementsExtractor` 只有 `extract_all()`（`extract_requirements.py:79`）[复核]；步骤 6-13 另有 ≥9 处接口断裂；`generate_content` 自动写入虚构业绩（"合同金额累计5000万元"）无阻断。
- **可复用（Apache-2.0）**：单脚本工具层（`parse_bid_file`/md 转换/`format_docx` 排版标准/字数公式/TOC 域/`industry_configs.json`）+ SKILL.md 骨架；`templates/` 27 个第三方范本有版权瑕疵不可搬。

### 3.9 Router0824/BidPilot-AI 【补充 · 仅借鉴（AGPL）】
- **形态**：FastAPI 服务端 + SQLite 30 表 + 自研 11 节点工作流状态机（38 后端测试 + 2 e2e + CI）。
- **纳入理由**：**唯一具备「失效传播增量重执行」设计**的仓库（上游变更 → 只重执行受影响下游）；`export_ready` 导出硬阻断 + CAS 内容版本保护 + 人工确认门——后端状态机的直接流程参照。
- **不可复用原因**：AGPL-3.0 且徽章误标 Apache；36k 字符静默截断；`blake2b` 伪向量检索；Mock 默认兜底常为模板文本。
- **可借鉴**：节点状态机定义、失效传播矩阵、证据链物化、人审门语义。

### 3.10 sunShineLoveMe/ai_bidding_power_grid 【补充 · 可直接复用】
- **形态**：Flask ~40 路由 + React 前端 + PG/pgvector（282MB 体积主要来自电力行业数据集），自述思想继承 `NewbieCoderLab/ai_bidding`。
- **纳入理由**：**唯一 9 环全实现的可运行系统（MIT）**：MinerU 优先 + >200 页拆分 + 断点续传；废标/否决 = 9 关键词纯规则子串识别 [复核]；双轨合规（规则+LLM）；公文导出/TOC 域/soffice 刷新；16 测试 + `ai_usage_logs` 成本视图。
- **可直接复用（MIT）**：`bid_volumes` 分册体系、`md_to_word` 公文导出、`001_schema.sql`、双轨合规骨架、`rag_seed` 索引三元结构（合成测试集模板）。
- **风险**：80/60/80 截断 + 段落截 500 字；纯子串废标识别（变体全漏、4 类互斥 break）；无服务端定稿门禁；**仓库内含真实电力行业招标数据，二次分发需审查**。

### 3.11 zeronezer/bidcraft 【补充 · 需改造复用】
- **形态**：LangGraph 编排（Apache-2.0，仅 2 次提交，思路文档 01-08 齐全）。
- **纳入理由**：**评估图编排框架的第一手证据**——实践中已把 `interrupt()` 人审门移除（v0.2 自动落库决策 [复核：`node_generate_toc` 自动落库，节点内无 `interrupt()` 调用]），图退化为线性两节点；PyMySQLSaver 检查点续跑真实可用但 `COMPILE_AUTO_RESUME` 默认关闭。
- **可复用（Apache-2.0 可闭源）**：`WRITER_SYSTEM` 硬规则、chat 定稿核验、三级漏斗选章、`file_index` 无 RAG 选章、fact 版本管理、PyMySQLSaver 接线范式、`init_db.sql`。
- **风险**：3 个 API 因缺 import 必 500；`sections.py:693` project_id 硬编码 0；400k 文本截断；测试为失效的人工探针。

### 3.12 aigc-open/tender-cli 【补充 · 需改造复用（MCP 范式）】
- **形态**：32 文件的 CLI 原型（Apache-2.0，停滞约一年），搜索发现的唯一真 FastMCP。
- **纳入理由**：MCP 接线范式的现实样本——`src/tender_cli/mcp_tools/base.py:8` `from mcp.server.fastmcp import FastMCP` 真实 [复核]；但依赖陷阱已验证：`base.py:81` 注解引用 `MCPServer`，而其导入在 `try: from agents import ...`（PyPI 包名 `openai-agents`）内，`pyproject.toml` 未声明该包 → 缺包时模块导入必抛 NameError [复核]；MCP server 仅挂载数学/字符串示例工具，标书业务工具从未经 MCP 暴露；`set_project_dir` 全仓零调用 → 导出必败。
- **可复用**：FastMCP + Agents SDK 接线范式、config 三级加载、`project_manager`、`file_tools`（修好后）。
- **结论**：作为「MCP 工具粒度与依赖声明」的反面教材 + 接线参考，价值中等。

### 3.13 kcaaaa/bid-writing-agent 【补充 · 仅借鉴（机制设计）】
- **形态**：**零代码纯提示词**——13 个 SKILL.md（frontmatter 全部合法 [复核]）。
- **纳入理由**：Skill 机制设计样本最全：12 子 Skill 分工、11 道人工门禁（"门禁"出现 48 次 [复核]）、三重熔断、6 状态 + version 锁 + 8 字段断点续写（`docs/bid-skill-design.md` 的直接参照）。
- **不可复用原因**：无 LICENSE → 任何文本不可复制；门禁/熔断零代码强制，全靠模型自律（历史自认四大故障：token 超限/显存爆炸/死循环/调用失败）。

### 3.14 mrwisdom2/biaoshuv2 【补充 · 不建议采用（法律红线）】
- **形态**：审校专用系统（非编写器）：docx 切分 → Qdrant/BM25/RRF/Cross-Encoder 混检 → Tech/Biz 双策略 LLM 判决 → 高亮回写（非原生批注）；2865 行测试真实。
- **纳入理由**：审校环最佳工程参照（策略模式判决、规则三写一致性、18 参数热调）。
- **不可复用原因**：无许可证 + **仓库含运达内部报价规则与国电投真实招标文件**（公开即合规红线）；判决 fail-open；Qdrant 故障时任务"completed"但 0 批注（静默失效）。

### 3.15 RudyJunyu/Bid_Agent 【补充 · 仅借鉴（clean-room）】
- **形态**：Python 单体，解析链最强（10 种解析方式、PDF 四类分流、MinerU 并行页段、表格行级切片）。
- **纳入理由**：解析数据结构最完整的样本——16 字段画像、34 项审查正则（仅 1 项涉废标）、chunk 五层 metadata、684 行表设计文档。
- **不可复用原因**：无许可证；**.chroma 15MB（2,460 向量）+ 19MB 业务库被误提交**（真实数据泄露）；页码列建了未回填。
- **结论**：只可 clean-room 借鉴数据结构与打分公式思路，禁止抄代码/提示词/规则表。

## 4. 初筛级仓库（未深度审计，证据等级 [初筛]）

| 仓库 | 星/许可 | 定位与结论 | 不深审原因 |
|------|---------|-----------|-----------|
| [Yohann1024/intelligent-presales-skill](audit-notes/screening-b.md) | 17 / 无 | 真实 Claude 式 SKILL.md + 5 脚本 1569 行（Ollama+ChromaDB 解析→检索→生成→Word/HTML），闭环但无矩阵/审校 | 价值与 Get00/ark 重叠，无许可证 |
| [tlyyxjz/BidAgent](audit-notes/screening-b.md) | 2 / Apache-2.0 | 业务是**中标公告核验**非标书编写；但 `evidence_locator`（字符级证据定位）、`field_validator`、`hallucination_checker` 可迁移到标书事实核查；27.9k 行 app + 28.5k 行测试 | 业务主体不属于标书链条，仅子系统有价值 |
| [William-GuoWei/ProposalLLM](audit-notes/screening-b.md) | 373 / Apache-2.0 | 「需求对应表.xlsx 驱动逐条应答 + 技术偏离表回填」最小实现（757 行），2024-11 停更 | 规模小且停更，思路已被 ai_bidding 系覆盖 |
| [samzhao84-code/bid-workbench、WangM-A3/nexus-bid、ZhangDongfang2006/bid-generator、qilin892/bid-supporting-materials 等](audit-notes/screening-a.md) | 1–5 / 不一 | 搜索命中的小型项目，规模与活跃度不足以进入深度审计 | 初筛即可判定复用价值有限 |

## 5. 排除清单与搜索噪声（记录筛选理由）

- **mag05270/BidAgents（10★）**：EXCLUDE——单次 init 提交、零测试；README 声称基于 LightRAG，但全后端无 `import lightrag`（仅 docstring/配置提及）——典型 README 技术栈虚标。
- **关键词噪声**：搜索"标书"命中的 `shellljx/TagViewGroup`（Android 小红书标签组件）、`Higurashi-kagome/pythontools`、`Warrenren/inside-rust-std-library`（技术书稿）、`chenzhixuanfy/PEP-Primary-School-*`（电子课本）、`bids-standard/bids-examples`（脑成像 BIDS 标准）等均为"标书/bid"字面撞车，已排除。
- **空壳/演示**：`dusiwan-dotcom/AI-Agent`（0KB）、`lushuping126/construction-technical-proposal-ai`（0KB）、`Zhuwenbopro/biaoshu_generator`（自述"生成垃圾标书"的玩笑项目）等排除。
- **相邻但非链条**：`tuke-workspace/RFP-AutoPilot`（B2B RFP 格式清洗，非投标生成）、`wangyunchen92/bid-management-system`（招投标管理平台重在前端演示）不纳入。

## 6. 许可证风险总览

| 风险类型 | 仓库 | 事实与证据 | 对本项目约束 |
|---------|------|-----------|-------------|
| README/manifest 虚标 | BidMaster-Pro [复核] | LICENSE 文件 = AGPL-3.0；README 第 7/375 行标 MIT | 以 LICENSE 文件为准；代码零复用 |
| README/manifest 虚标 | BidPilot-AI | LICENSE = AGPL-3.0；README 徽章 Apache-2.0 | 同上 |
| manifest 虚标 | tender-cli [复核] | LICENSE = Apache-2.0；pyproject `license={text="MIT"}` + classifier MIT | 以 LICENSE 为准（Apache-2.0），依赖声明需修正后再用 |
| AGPL 历史窗口 | LingYan-Bid-Agent [复核] | 2026-09-10 引入 AGPL-3.0-only，09-16 切回 MIT；当前 HEAD 为 MIT | 仅从 HEAD 复用；禁止 cherry-pick 09-10~09-16 区间提交 |
| AGPL 直接限制 | OpenBidKit_Yibiao、BidMaster-Pro、BidPilot-AI | 均为 AGPL-3.0 | 设计思路可学（clean-room），代码/提示词/配置一律不可复制 |
| AGPL 传染依赖 | 96syh/biaoshu | 默认依赖 PyMuPDF（AGPL） | 复用其代码时必须替换解析器为 pdfium/MinerU（Apache-2.0）等 |
| 无许可证 | ark-ai-bid-agent、llm_bidwriting、bid-writing-agent、biaoshuv2、RudyJunyu/Bid_Agent | 无 LICENSE 文件 = 默认版权保留 | 代码/提示词/规则文本一律不可复制；仅允许独立重写思路 |
| 真实数据泄露 | biaoshuv2（运达报价规则+国电投招标文件）、RudyJunyu/Bid_Agent（.chroma 15MB + 19MB 库）、ai_bidding_power_grid（电力行业数据集）、llm_bidwriting（git 历史业务数据） | 审计已确认 | 这些数据/从中学习的具体规则**不得**进入我们的系统或提示词 |

## 7. 横向结论（对架构设计的直接输入）

1. **生态空白**：15 个深度审计仓库中，**没有一个**同时具备「Skill + 业务级 MCP 工具 + 状态化后端」——真 Dify DSL 仅 1 家（且为最弱实现）、真 MCP 2 家（示例级/静默 stub）、真 Skill 5 家。目标架构在开源生态中无现成可搬组合，业务 MCP 层必须自建。
2. **截断丢条款是系统性缺陷**：12k（ai_bidding）~400k（bidcraft）各种字符上限、36k~80k 分块静默截断、无一个仓库具备「条款完整性校验」（证明没有丢失）。这是全链条最优先要解的可靠性问题。
3. **硬阻断几乎全军覆没**：仅 ai_bidding 有 4 处服务端硬门（事实门/模板409/空章禁导出/占位文本）与 96syh 有 400 字人工确认门，其余阻断要么仅前端（ark、power_grid）、要么靠警示（LingYan）、要么没有（OpenBidKit 高风险 findings 不阻止导出）。**门必须放进服务端状态机**。
4. **恢复能力两极分化**：真实恢复 = LingYan（checkpoint+断点续跑）、OpenBidKit（8 相位）、ai_bidding v2（跳过已生成+事件回放）、BidPilot（SQLite resume）；假恢复 = BidMaster（只存 key 名，恢复即全量重跑）；图框架恢复 = bidcraft（真但默认关闭且图已退化）。失效传播（补遗后识别受影响章节）仅 BidPilot 有设计。
5. **确定性检查的边界共识**：占位符/字数/查重/关键词废标/格式规则普遍用确定性代码；报价算术、资质有效期、签章、暗标没有一家做全——OpenBidKit 的 64 规则 + BlindCheck 是唯一接近完整的样本。
6. **测试与成熟度倒挂**：成体系测试仅 6 家（96syh、LingYan、ai_bidding、power_grid、BidPilot、biaoshuv2）；星标高低与工程质量无关（3202★ 的 OpenBidKit 无 CI 跑测试；406★ 的 BidMaster 零测试且虚标许可证）。
7. **工具粒度教训**：tender-cli 证明「MCP 只挂示例工具」= 没有业务价值；OpenBidKit 的 C# 内核「单行 JSON + 任务目录」与 ai_bidding 的「五域上下文 + 事件溯源」是工具粒度设计的两个正面参照。

