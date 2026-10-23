# 真实执行链路审计（workflow-audit）

> 证据来源：`research/audit-notes/` 下 15 份逐仓库源码审计报告（每个入口均给出文件+函数级证据，此处为压缩视图）。
> 链路判定口径：**完整链** = 输入→解析→条款→证据→矩阵→目录→分章→审校→导出 9 节点全部有真实实现；**部分链** = 主干可跑但节点缺失/失效；**提示词链** = 仅提示词声明无代码；**无链** = 无可执行链路。
> 标注 [复核] 的结论已经主代理二次验证（脚本见 `scripts/spotcheck.py`）。

## 0. 理想链 9 节点 × 15 仓库覆盖总览

| 链路节点 | 真实实现（IMPLEMENTED） | 部分/失效 | 完全缺失 |
|---------|------------------------|----------|---------|
| ① 输入/文件接入 | ark、96syh、LingYan、BidMaster、OpenBidKit、power_grid、bidcraft、RudyJunyu | ai_bidding（无表格/OCR）、BidPilot（表格丢）、tender-cli（不可达）、Get00、llm_bidwriting（500K不分块） | kcaaaa（纯提示词） |
| ② 解析（含中间结构） | LingYan（页锚点）、RudyJunyu（行级切片）、OpenBidKit（双通道表格）、power_grid（>200页拆分） | 其余多数（截断/丢表格） | — |
| ③ 六类条款抽取 | ark（双轨）、96syh（5角色）、power_grid（规则+LLM） | LingYan（七节）、ai_bidding（无ID）、BidPilot（36k截断）、Get00（步骤3崩[复核]） | OpenBidKit（无统一schema）、tender-cli（:10截断） |
| ④ 证据检索 | ai_bidding（混合检索+score_trace） | ark（双路+过期过滤）、power_grid（仅语义） | 其余大半（词法-only / LIKE / 空壳） |
| ⑤ 响应矩阵 | 96syh、power_grid（覆盖矩阵）、ark（逐条判定） | BidPilot（五态启发式）、LingYan（dangling） | ai_bidding（死码[复核]）、OpenBidKit、Get00、bidcraft |
| ⑥ 目录生成 | 96syh、ai_bidding（确定性格式链）、LingYan（规定目录优先） | BidPilot（模板+LLM）、bidcraft（自动落库） | tender-cli（硬编码7章） |
| ⑦ 分章节生成（任务化） | ai_bidding v2、OpenBidKit、LingYan、bidcraft、power_grid | ark（固定4模块）、BidPilot（每run 1章） | Get00（样板文本）、kcaaaa（SPEC_ONLY） |
| ⑧ 审校/校验 | OpenBidKit（64规则+查重）、biaoshuv2（混检+判决） | ai_bidding（4硬门）、LingYan（占位+承诺比对）、RudyJunyu（34正则） | bidcraft（质检已移除）、Get00（仅字数） |
| ⑨ 导出 | OpenBidKit（C#内核/暗标）、LingYan（docx内核3645行）、ai_bidding、power_grid（公文）、96syh | BidMaster（LibreOffice降级）、BidPilot | bidcraft（roadmap）、RudyJunyu（Markdown blob）、tender-cli（必败） |

**结论**：9 节点全部为 IMPLEMENTED 的仓库 = **0**。最接近的是 power_grid（8/9，缺服务端定稿门）与 96syh（8/9，证据检索弱）。

## 1. 逐仓库真实链路（压缩视图）

### 1.1 anlaiye/ark-ai-bid-agent —— 完整链（工作台形态）
```
上传(PDF/DOCX/XLSX/CSV/TXT) → 解析[parsers.js#extractText，扫描件显式拒绝]
→ 条款抽取[generator.js：LLM+正则双轨互验，六分类+评分独立表；70k字符截断/120条上限]
→ 证据检索[Fastify RAG服务：pgvector语义 ∥ bigram词法双路 + 主体冲突剔除 + 过期SQL硬过滤]
→ 响应矩阵[response_matrix 逐条判定] → 目录[固定4模块] → 分章生成[模块级]
→ 审校[确定性分级+LLM判定] → 导出[ExcelJS + docx]
```
持久化：PG 10 表 + 版本快照。缺失/失效：单章重试、条款页码锚点（source_location 为 LLM 自报）、**导出硬阻断仅前端**（`server.js:331-347` API 不校验冲突）。

### 1.2 96syh/biaoshu —— 完整链 8/8
```
上传 → 解析[pdfplumber/PyMuPDF/PyPDF2/docx2python + MinerU可选]
→ 条款抽取[5角色Agent → AnalysisReport：40+数据模型/ID体系；>45k长文关键词窗口裁剪]
→ 证据检索[仅词元加权打分] → 响应矩阵[真实] → 目录[确定性直出+LLM双路径]
→ 分章[单章重试] → 审校[LLM内容检查 + 占位符清除（代码级仅此一项）]
→ 导出[python-docx + TOC域；manual_review_confirmed 400字硬门]
```
持久化：SQLite 整草稿。测试：62 后端 + 10 前端。风险：`ready_to_export=false` 不在代码层拦截导出。

### 1.3 joychin/LingYan-Bid-Agent —— 完整链（重工程，溯源/恢复最强）
```
输入 → 解析[pdfium四档结构 + 百度OCR扫描件 + 页锚点]
→ 条款抽取[七节分类 + 机器校验 + 子代理并发] → 溯源[引用键 + 行号/页码校验]
→ 证据检索[FTS5+jieba，无向量] → 矩阵[registry+lineage 未接线，存在dangling]
→ 目录[招标规定目录优先] → 分章[并发写节 + 重派守卫(AIMD)]
→ 审校[占位硬禁 + shingle查重 + 承诺比对（确定性）] → 导出[自研docx内核3645行：修订标记/元素级注入/合册]
```
恢复：app.db + agent.db checkpoint + 断点续跑。测试：67 文件 2.1 万行。风险：抽取内容真伪无机器校验；deepagents==0.7.7 强锁定。

### 1.4 Chengyihaowen/llm_bidwriting —— 部分链（Dify 外挂式轻全栈）
```
上传 → 解析[pdfplumber 全文拼接，500K上限，不分块]
→ 条款抽取[Dify DSL「招标文件解析工作流」→ 落 TEXT blob]
→ 证据检索[中文2-gram 词法TF，无语义] → 矩阵[缺失]
→ 目录[OutlineNode] → 分章[GenerationTask + SSE 逐章，可编辑]
→ 审校[Dify DSL「废标检查工作流」——但"当前标书正文"误绑 parsed_bid，检查从未见过标书正文]
→ 导出[python-docx 单模板]
```
**唯一真 Dify DSL×3** [复核]（解析/章节生成/废标检查，YAML 可解析、`app.mode=workflow`）；需修变量注入 bug + 花括号规范 + 配模型插件。风险：目录重存 SQL 批量 delete 孤儿化章节；检查工作流无 timeout 可永久挂起。

### 1.5 guangshu100/BidMaster-Pro —— 完整链（演示级）
```
输入 → 解析[pdfplumber/MinerU OCR 真实；表格丢弃] → 解读[15维 LLM，无统一条款清单]
→ 证据检索[向量+TF-IDF RRF，2处bug] → 矩阵[仅评分项→章节 n-gram]
→ 大纲 → 分章[逐章生成，可重试，SSE] → 检查[15项并行 = LLM提示词 + 32k截断]
→ 自查 → 排版[公文排版885行 + LibreOffice三级降级] → 导出[docx，无暗标]
```
恢复：checkpoint 只存 key 名 → resume 即全量重跑（**假恢复**）。零测试 + 7 处依赖错配；README 虚标 MIT [复核：LICENSE=AGPL-3.0]。

### 1.6 NewbieCoderLab/ai_bidding —— v2 分册链完整（矩阵死码）
```
输入 → 解析[PyPDF2/mammoth 纯文本 + 正则格式章节识别；12k预分析截断]
→ 条款抽取[LLM单JSON；资格/否决/格式有字段，无条款ID]
→ 证据检索[PGVector + FTS + trgm + rerank降级 + MySQL LIKE兜底；score_trace留痕]
→ 矩阵[mark_response_matrix_coverage 全库零调用 = 死码 [复核]；_check_scoring_coverage 评分级真实(api/bidding.py:1130)]
→ 目录[tender_format_parser 确定性格式链] → 分章[v2：单章任务/版本/restore/续跑]
→ 审校[4处服务端硬门：事实门/模板409/空章禁导出/占位文本]
→ 导出[md→docx 中文排版 + 四级流程图降级 + OnlyOffice]
```
持久化：MySQL 43 表 + 事件回放。测试：57 例（离线可跑）。

### 1.7 FB208/OpenBidKit_Yibiao —— 主链完整（桌面端工程标杆）
```
输入 → 解析[mammoth/pdfjs双通道表格 + MinerU两API；扫描件显式报错引导]
→ 18项解析[含 无效投标/废标项 专项目录；无统一条款schema/ID]
→ 凭证库[SQLite + 知识库 instr 关键词检索] → 矩阵[缺失]
→ 目录[Pi Agent 目录树 + 级联失效协议] → 分章[并发生成 + 单章重试/失败重试/resume]
→ 审校[64条C#确定性规则 + 查重四维算法 + LLM双轨；无硬阻断]
→ 一致性/表格 → 版面 → 导出[C# render-restricted-html-docx；整本/模板sdt/暗标修复副本/Excel]
```
恢复：8 相位 + inputSignature 缓存。风险：高风险 findings 不阻止导出；硬编码客户串（`duplicateCheckService.cjs:1893`）。AGPL。

### 1.8 Get00/BiaoShu-SKILL —— 部分链（Skill 驱动，主流水线必崩）
```
SKILL.md(805行) 引导宿主Agent → parse_bid_file[多格式+扫描检测，无OCR]
→ extract_scoring[评分标准] → extract_requirements[**步骤3必崩**：bid_writer_pipeline.py:473 调 .extract()，而 RequirementsExtractor 仅有 extract_all() [复核]]
→ （步骤6-13 另有 ≥9 处接口断裂）→ generate_content[6套硬编码样板，含自动虚构业绩"合同金额累计5000万元"]
→ format_docx[3套排版标准+TOC域] → 导出
```
证据检索（company_profile 死参数）/矩阵/状态恢复缺失；字数公式检查为唯一审校。Apache-2.0 工具层可复用。

### 1.9 Router0824/BidPilot-AI —— 部分链（11 节点状态机真实）
```
输入 → 解析[pdfplumber/docx/xlsx；无OCR、表格内容丢；≤36k字符静默截断]
→ 条款抽取[六类LLM分块；无废标类型] → 溯源[LLM页码 + 规则兜底伪页码]
→ 证据链[RequirementEvidenceLink 五态启发式；blake2b伪向量+关键词检索]
→ 矩阵[评分覆盖死字段] → 目录[模板+LLM，CAS版本保护] → 分章[每run仅1章]
→ 审校[4类纯确定性检查] → 导出[DOCX模板/TOC真；"xlsx"实为TSV]
```
状态机：11 节点 + **失效传播（上游变更→下游增量重执行）** + export_ready 硬阻断 + 人审确认门；SQLite 30 表 + resume；38 测试 + CI。AGPL。

### 1.10 sunShineLoveMe/ai_bidding_power_grid —— 完整链 9 环（MIT 参照系）
```
上传 → 解析[MinerU优先 + >200页拆分 + 断点续传]
→ 条款抽取[bid_interpreter：废标=9关键词纯子串规则 [复核] + LLM分段解读；80/60/80截断+段落截500字]
→ 溯源[40字snippet → page_idx 弱锚点] → 证据[pgvector+rerank语义；无编号/金额/日期精确匹配]
→ 覆盖矩阵（非应答矩阵）→ 分册[bid_volumes 规则骨架 + 后台AI精化防退化]
→ 章节生成[单章流式 + 补写pass] → 合规[双轨：规则 + semantic_compliance LLM复核；无硬阻断]
→ 导出[md_to_word 公文格式/TOC域/soffice刷新；无暗标]
```
15 表 DDL + 任务表 + 状态 JSON；`ai_usage_logs` 成本视图；16 测试。风险：定稿门禁仅前端弹窗可跳过。

### 1.11 zeronezer/bidcraft —— 部分链（LangGraph 实践样本）
```
输入 → 解析[MinerU 200页自动切块 / docx/xlsx直读]
→ 抽取[招标整体4类+标段+规定目录；text[:400k]截断+24k输出上限]
→ 事实[三级漏斗+AI选章+完整原文，no-RAG；fact版本管理]
→ 目录[compile_graph 节点1：自动落库（v0.2，interrupt人审门已移除 [复核]）]
→ 思路[节点2：逐章编写思路] → 逐章正文[writer_service 按章触发/可停/重写] → 对话改稿闭环
→ 审校[缺失：质检智能体 2026-09-10 移除] → 导出[缺失：roadmap]
```
LangGraph PyMySQLSaver 检查点真实（`invoke(None, thread_id)` 续跑）但 `COMPILE_AUTO_RESUME` 默认 false；图已退化为线性两节点。

### 1.12 aigc-open/tender-cli —— 部分链（多处断裂）
```
对话 → 建项目[project.json] → 解析[PyPDF2 全文；全路径不可达]
→ 条款[正则六字段，:10/:15 硬截断，否决/格式缺失] → 大纲[硬编码7章]
→ 内容[固定模板非模型产出] → 审校[validate_outline 规则存在但断链]
→ 导出[docx 实现真实但 set_project_dir 全仓零调用 → 必败；PDF=改名stub]
MCP旁路：base.py:8 FastMCP 真实 [复核]，仅挂数学/字符串示例工具；
mcp_agent 依赖 pydantic-ai；base.py:81 注解引用 MCPServer（导入在 try 块内，openai-agents 未声明于 pyproject → 缺包 import 必崩 [复核]）
```

### 1.13 kcaaaa/bid-writing-agent —— 全链 SPEC_ONLY（提示词声明链）
```
逐章扫描(OCR疑点保守标记) → 八类五要素需求 → 硬件参数 → 大纲(招标映射字段)
→ 防重 → 任务拆分 → 分章撰写(字数熔断) → 流式审查 → OfficeCLI 导出
机制：6状态 + version锁 + 8字段断点续写；11道人工门禁；三重熔断（token超限/显存/死循环/调用失败各有降级清单）
——全部为零代码强制的提示词设计（13 个 SKILL.md frontmatter 合法 [复核]）
```

### 1.14 mrwisdom2/biaoshuv2 —— 审校链真实（无生成链）
```
上传 → 解析[docx段落+嵌套表格切分；.doc虚假支持] → 锚点[DocumentAnchor 整型段落/单元格级]
→ 混合检索[Qdrant/BM25/RRF + 四层过滤 + Cross-Encoder精排]
→ LLM判决[Tech/Biz双策略，temp=0.01，fail-open] → 规则匹配[规则库三写一致性]
→ 批注回写[整段标黄+红字段落，非Word原生批注] → OSS下载
```
MySQL 任务状态机无断点续跑；Qdrant 故障 → 门控拦截全部 → 任务 completed 但 0 批注（静默失效）；2865 行测试真实但 446 行 API 测试过期。**仓库含真实企业数据，不建议采用。**

### 1.15 RudyJunyu/Bid_Agent —— 部分链（解析最强）
```
输入 → 解析[bid_parser_api：10种方式/PDF四类分流/MinerU并行页段/表格行级切片(标准Markdown表)]
→ 召回[FTS5+Chroma混合打分] → 两阶段上下文 → 抽取[五模块LLM并发；废标挂资格模块]
→ 入库[23表；页码列建而未回填] → 审查[content_review_agent：34项正则(仅1项涉废标)+溯源评分]
→ 目录[规则生成] → 单章草稿[不持久化] → 导出[Markdown blob]
```
无许可证 + .chroma 15MB/19MB 业务库误提交。企业证据仅 LIKE 检索且表空壳。

## 2. 十问综合回答（跨仓库证据汇总）

### Q1 超长招标文件/附件/表格/扫描件：如何防截断、解析失败、分块遗漏丢条款？
**结论：没有一个仓库具备"条款完整性校验"，截断丢条款是本次审计最大的系统性缺陷。**
- 截断上限实测谱系：ai_bidding 12k 预分析、BidMaster 32k、BidPilot 36k、96syh 45k 关键词窗口、ark 70k/120条、power_grid 80/60/80 + 段落截 500 字、bidcraft 400k、llm_bidwriting 500K 不分块。
- 正面样本：LingYan（pdfium 四档结构 + 页锚点 + 机器校验）、OpenBidKit（MinerU + inputSignature 缓存 + 8 相位恢复）、RudyJunyu（MinerU 并行页段 + 行级切片）、power_grid（>200 页拆分 + 断点续传）。
- 对目标架构的输入：**结构化分块（按章/节边界）→ 逐块抽取清单 → 覆盖率核对（分块数×抽取数必须对账）→ 否决项 100% 召回硬门 → 扫描件显式拒绝并转 OCR 子任务，不静默降级**。

### Q2 六类要求如何统一成结构化条款清单？
- 分类实现谱系：关键词+章节开关（Get00）、七节+机器校验（LingYan）、5 角色 Agent→40+ 模型（96syh）、六分类双轨互验（ark）、15 维解读（BidMaster）、4 类（bidcraft）、五模块（RudyJunyu）、9 关键词子串（power_grid）、Dify 提示词段落（llm_bidwriting）。
- **共性缺陷：无一家有跨重跑稳定的条款 ID**（ai_bidding 无 ID；RudyJunyu 页码列建而未填；补遗/重跑后无法对齐新旧清单）。
- 设计输入：`REQ-ID = hash(规范化文本 + 原文锚点)`，六类必填枚举 + 数值/日期字段规范化抽取；补遗走增量抽取 + 与既有清单做内容对齐匹配。

### Q3 条款如何关联原文位置、证据、响应章节、审校结果？
- 溯源最强：LingYan（引用键 + 行号/页码**机器校验**）；其余为弱锚点：power_grid（40 字 snippet→page_idx）、biaoshuv2（整型段落/单元格锚点）、OpenBidKit（暗标段落定位器）、RudyJunyu（chunk_id 仅 Schema 模式）。
- **四向关联（条款↔证据↔章节↔审校）无一家完整实现**。部分实现：ark `response_matrix` 逐条判定；ai_bidding CIT 证据登记（仅输入侧）。
- 设计输入：统一 ID 体系（REQ-/EV-/CMP-/SEC-/FIND-/TASK-）+ 关联表 + 审计轨迹；文档内核块 ID 独立命名空间，绝不与条款/证据 ID 混用。

### Q4 语义召回与精确匹配是否兼得？
- 最接近双通道：ark（pgvector 语义 ∥ bigram 词法 + **过期日期 SQL 硬过滤**——唯一做证据时效的仓库）；工程最完整：ai_bidding（PGVector + FTS + trgm + rerank 降级链 + LIKE 兜底 + score_trace）。
- 其余为单一通道：power_grid 仅语义（编号/金额/日期精确匹配无）、96syh/LingYan 词法、RudyJunyu LIKE、OpenBidKit instr 子串。
- 设计输入：**三通道混合检索** = 向量语义 + FTS/trgm 词法 + 结构化字段过滤器（证号精确等值、金额区间、日期有效期、型号规格归一），后置 rerank；所有证据携带 validFrom/validUntil。

### Q5 无证据/过期/冲突/资格不满足时，如何阻止错误内容进入定稿？
**结论：硬阻断几乎全军覆没，且"看起来有门"的仓库大多可绕过。**
- 服务端真硬门：ai_bidding 4 处（事实门/模板 409/空章禁导出/占位文本）+ BidPilot `export_ready`（AGPL 思路可学）。
- 可绕过：96syh 400 字人工确认门（API 层不拦 ready_to_export=false）、ark 导出阻断仅前端、power_grid 定稿弹窗仅前端可跳过。
- 无阻断：OpenBidKit（高风险 findings 不阻止导出）、LingYan（过期/不满足仅警示）、Get00（虚构业绩直接写入）。
- 设计输入：**阻断必须落在服务端状态机**——BLOCKED 状态 + 人工 waiver（记录决策人/理由/有效期）才能流转，导出前置校验在服务端复核一遍。

### Q6 章节能否拆成可单独执行/重试/修改/验证的任务？
- 这是全链条解决得最好的环节：ai_bidding v2（单章任务/版本/restore/续跑）、OpenBidKit（单章重试 + 级联失效协议）、LingYan（并发写节 + AIMD 重派守卫）、bidcraft（可停/重写/对话改稿）、power_grid（单章流式 + 补写 pass）。
- 设计输入：章节=任务实体（attempts/maxAttempts/校验状态/依赖章节），支持 CAS 版本保护，审校发现自动生成 RevisionTask 挂回章节。

### Q7 报价/工期/数量/资质有效期/签章/暗标：确定性代码 vs 模型？
- 确定性代码已覆盖：占位符禁令（ai_bidding）、字数/公式（Get00）、查重四维（OpenBidKit）、64 C# 规则（OpenBidKit）、34 正则审查项（RudyJunyu）、9 关键词废标（power_grid）、4 类格式检查（BidPilot）、承诺比对 + shingle（LingYan）、暗标 BlindCheck（OpenBidKit，唯一）。
- 全线缺失：报价算术一致性、资质有效期硬校验（除 ark 过期过滤）、签章/盖章检查——**没有任何仓库做全这六项**。
- 设计输入：**规则注册表（可执行、可测试）**承载一切确定性检查；原则"代码能查的绝不交给模型"；模型只负责语义合规与内容质量判断。

### Q8 中断/换模型/补充招标文件后如何恢复？
- 真实恢复：LingYan（checkpoint + 断点续跑 + AIMD）、OpenBidKit（8 相位 + inputSignature）、ai_bidding v2（跳过已生成 + 事件回放）、BidPilot（SQLite resume + **失效传播**——唯一有"识别受影响章节并增量重执行"设计的仓库）。
- 假恢复：BidMaster（checkpoint 只存 key 名，resume = 全量重跑）。
- 图框架恢复：bidcraft PyMySQLSaver 真实可用但默认关闭，且图已退化、人审门已移除 [复核]。
- 换模型场景：无一家有方案（BYOK 配置是有的，但无"换模型后哪些章节需重验"的追踪）。
- 设计输入：**事件溯源 + 状态机 + 失效传播矩阵**；补遗导入自动触发受影响条款的重抽取与下游章节的 invalidation；模型版本记入生成审计，换模型只影响未定稿章节。

### Q9 可导入 Dify DSL / 可运行 MCP Server / 独立 Skill 是否真实存在？
- **Dify DSL**：全调研唯一 = llm_bidwriting，3 份（`docs/dify_constronstor/*.yml`），YAML 可解析、含完整图定义 [复核]；但需修变量注入 bug、花括号规范、配模型插件，且废标检查存在变量错绑缺陷。
- **MCP Server**：2 家真协议实现、0 家可用业务工具——tender-cli（`base.py:8` FastMCP 真实 [复核]，仅示例工具，openai-agents 依赖未声明 → import 崩 [复核]）、BidMaster-Pro（官方 FastMCP 8 工具，依赖错配 → 静默退化为 Stub）。
- **独立 Skill**：5 家真实——ark（SKILL.md + 201 行 stdlib 校验器 + evals，工程最完整）、LingYan（6 个，绑定 deepagents 运行时）、Get00（805 行，文档与脚本 ≥10 失配）、kcaaaa（13 个纯提示词）、Yohann1024（初筛级）。
- **结论：生态空白确认**——"业务级 MCP + Skill + 状态化后端"组合在开源界不存在现成可搬资产，必须自建，但协议级/结构级参照充分。

### Q10 哪些可合法复用，哪些仅可借鉴？
- **可直接复用（代码级，许可证干净）**：ai_bidding（MIT：格式目录链 4 文件、混合检索、md_to_word、v2 事件溯源）、power_grid（MIT：分册体系、公文导出、schema、rag_seed）、96syh（MIT：schema_defs、prompts——须剔除 PyMuPDF/AGPL）、Get00（Apache：工具脚本层 + SKILL 骨架）、bidcraft（Apache：WRITER_SYSTEM、三级漏斗、PyMySQLSaver 接线）、tender-cli（Apache：FastMCP 接线范式）、LingYan（限 HEAD/MIT：解析器、docx_ops、skills）。
- **仅可借鉴思路（clean-room）**：OpenBidKit / BidPilot / BidMaster（AGPL）、ark / llm_bidwriting / kcaaaa / biaoshuv2 / RudyJunyu（无许可证）。
- **不建议采用**：BidMaster-Pro（AGPL+虚标+零测试）、biaoshuv2（无许可证+真实企业数据）。
- **共同红线**：从泄露数据中学到的具体规则/内容不得进入我们的提示词与系统。

## 3. 缺口 → 目标架构要求映射

| 审计证实的能力缺口 | 对目标架构的节点级要求 | 落点文档 |
|-------------------|---------------------|---------|
| 条款静默丢失（无对账机制） | 逐块抽取清单 + 覆盖率对账 + 否决项 100% 召回门 | `test-plan.md`、`bid-skill-design.md` |
| 无稳定条款 ID | REQ-ID = hash(规范文本+锚点)；补遗增量匹配 | `bid-domain-model.md` |
| 门禁可被前端/API 绕过 | 服务端状态机硬门 + waiver 流程 | `bid-mcp-contract.md` |
| 语义/精确检索缺一 | 三通道混合检索 + 结构化字段过滤器 | `bid-mcp-contract.md` |
| 假恢复/恢复默认关闭 | 事件溯源 + 断点续跑默认开启 + 失效传播矩阵 | `bid-domain-model.md` |
| 虚构业绩/无证据断言 | 证据先行原则 + 生成后引用回查 + 占位符禁令 | `bid-skill-design.md` |
| 工具粒度失衡（示例工具/过细） | 8 个业务级工具 + 4 个辅助工具，拒绝细粒度暴露 | `bid-mcp-contract.md` |
| 确定性检查散落 | 规则注册表：金额/日期/数量/有效期/签章/暗标全覆盖 | `bid-domain-model.md`、`test-plan.md` |
