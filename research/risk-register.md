# 可靠性风险登记册（risk-register）

> 来源：15 份仓库源码审计（`research/audit-notes/`）中提取的可靠性缺陷 + 对本项目设计的风险推演。
> 严重程度：**严重** = 可导致废标/合规事故或法律侵权；**高** = 可导致条款丢失、错误定稿或功能失效；**中** = 影响可维护性/可观测性；**低** = 体验问题。
> 每条风险给出：证据位置（文件/函数）+ 复现或验证方法。标注 [复核] 的经主代理二次验证。

## 1. 开源仓库风险台账（审计证实）

### 1.1 条款完整性（截断与丢失）

| ID | 仓库 | 风险 | 严重度 | 证据 | 复现/验证方法 |
|----|------|------|--------|------|--------------|
| R-01 | ai_bidding | 12k 字符预分析截断 + 无表格/OCR 解析 → 长招标尾部条款静默丢失 | 高 | 解析服务预分析逻辑（见审计报告 §2a） | 构造 >12k 字符含尾部否决项的 PDF，检查抽取结果是否缺失 |
| R-02 | ark-ai-bid-agent | 70k 字符截断 + 120 条上限，超长部分仅正则兜底 | 高 | `generator.js` 抽取上限 | 同上，尾部埋 3 条否决项 |
| R-03 | BidMaster-Pro | 检查基于 32k 截断文本 → 长标书条款必漏检 | 高 | 检查节点输入构造 | 生成 >32k 标书正文，植入第 40k 字符处的废标项 |
| R-04 | BidPilot-AI | LLM 分块 ≤36k 静默截断；表格/扫描件内容丢 | 高 | 抽取分块逻辑 | 含大表格 + 扫描页的招标文件对照抽取结果 |
| R-05 | power_grid | 80/60/80 截断 + 段落截 500 字 | 高 | `bid_interpreter.py` 输入构造 | 500+ 字长段落内嵌否决条件 |
| R-06 | bidcraft | `text[:400k]` 硬截断 + 24k 输出上限 | 高 | 招标整体抽取输入 | >400k 输入文件尾部抽查 |
| R-07 | 96syh/biaoshu | >45k 长文关键词窗口裁剪，无遗漏告警 | 高 | 窗口裁剪逻辑 | 尾部条款 + 告警日志断言 |
| R-08 | llm_bidwriting | pdfplumber 全文拼接 500K 上限、不分块 | 高 | 解析入口 | 超限文件行为观察 |
| R-09 | Get00/BiaoShu-SKILL | **主流水线第 3 步必崩**（`extract()` 不存在）[复核] | 严重 | `bid_writer_pipeline.py:473` vs `extract_requirements.py:79` | 直接运行 `bid_writer_pipeline.py`（任何输入） |
| R-10 | 全体 | **无一个仓库具备条款完整性校验**（分块对账/召回率断言） | 高 | 各审计报告 Q1 均为否定 | 以已知条款清单的合成招标文件测召回 |

### 1.2 阻断与门禁失效

| ID | 仓库 | 风险 | 严重度 | 证据 | 复现/验证方法 |
|----|------|------|--------|------|--------------|
| R-11 | ark-ai-bid-agent | 导出硬阻断仅前端；API 端点不校验冲突可直接定稿 | 高 | `server.js:331-347` | curl 直接调导出 API 携带冲突标记 |
| R-12 | 96syh/biaoshu | `ready_to_export=false` 不在代码层拦截导出 | 高 | 导出路由 | 构造未确认状态调用导出 |
| R-13 | OpenBidKit_Yibiao | 高风险 findings 不阻止导出（无门禁） | 高 | 检查→导出链路 | 注入废标级 finding 后导出 |
| R-14 | power_grid | 定稿门禁仅前端弹窗，可跳过 | 高 | 前端确认组件 | API 直调导出 |
| R-15 | LingYan-Bid-Agent | 资格不满足/证据过期仅警示，无硬阻断 | 高 | 审校节点 | 过期证据用例 |
| R-16 | Get00 | `generate_content` 自动写入虚构业绩（"合同金额累计5000万元"）无阻断 | 严重 | 样板生成模板 | 断网/无企业资料时生成正文 |
| R-17 | biaoshuv2 | 判决 fail-open（异常判"合规"）；Qdrant 故障 → 任务 completed 但 0 批注 | 高 | 判决与门控逻辑 | 断开 Qdrant 后跑任务 |
| R-18 | RudyJunyu/Bid_Agent | LLM 抽取失败静默降级为"未抽取的候选片段" | 高 | 降级路径 | Mock LLM 失败注入 |

### 1.3 状态、恢复与数据一致性

| ID | 仓库 | 风险 | 严重度 | 证据 | 复现/验证方法 |
|----|------|------|--------|------|--------------|
| R-19 | BidMaster-Pro | checkpoint 只存 key 名，resume = 全量重跑（假恢复） | 高 | checkpoint 结构 | 中断后恢复，观察是否重跑已完成章节 |
| R-20 | bidcraft | 恢复真实存在但 `COMPILE_AUTO_RESUME` 默认关闭；人审门已被移除成死路径 [复核] | 中 | `compile_graph.py` / 配置 | 重启进程观察默认行为 |
| R-21 | llm_bidwriting | 目录重存/重解析 SQL 批量 delete 孤儿化已生成章节内容 | 高 | 目录保存路由 | 二次保存目录后查章节表 |
| R-22 | tender-cli | `set_project_dir` 全仓零调用 → 一键导出所有路径必败 | 高 | 全库调用点搜索 | 运行导出命令 |
| R-23 | ai_bidding | 响应矩阵写入函数全库零调用（死码）；覆盖率恒 0 [复核] | 中 | `mark_response_matrix_coverage` 调用点搜索 | 跑矩阵功能并断言非零 |
| R-24 | bidcraft | `requirement_service` 缺 import → 3 个 API 必 500；`sections.py:693` project_id 硬编码 0 | 高 | AST/调用分析 | 直接调用对应路由 |
| R-25 | llm_bidwriting | 检查工作流漏传 timeout → 可永久挂起 | 高 | `run_check_workflow` | 慢模型 Mock |
| R-26 | LingYan-Bid-Agent | `deepagents==0.7.7` 强绑定，升级即碎 | 中 | 依赖声明 | 版本升级冒烟 |
| R-27 | RudyJunyu/Bid_Agent | 页码溯源列建而未回填 → 溯源不可用 | 高 | 入库 schema vs 写入路径 | 抽取后检查 page 字段 |

### 1.4 法律与数据合规

| ID | 仓库 | 风险 | 严重度 | 证据 | 复现/验证方法 |
|----|------|------|--------|------|--------------|
| R-28 | BidMaster-Pro | README 虚标 MIT，LICENSE 实为 AGPL-3.0 [复核]——误导复用者 | 严重 | LICENSE 文件 vs README 第 7/375 行 | `head LICENSE` + README 对照 |
| R-29 | BidPilot-AI | 徽章 Apache-2.0，LICENSE 实为 AGPL-3.0 | 严重 | LICENSE vs README 徽章 | 同上 |
| R-30 | tender-cli | pyproject 标 MIT，LICENSE 文件为 Apache-2.0 [复核] | 中 | pyproject `license` 字段 | 同上 |
| R-31 | LingYan-Bid-Agent | 2026-09-10~09-16 存在 AGPL-3.0-only 窗口 [复核]，cherry-pick 该区间即传染 | 高 | `git log --follow LICENSE` | 复核命令即本方法 |
| R-32 | 96syh/biaoshu | 默认依赖 PyMuPDF（AGPL）传染 | 高 | requirements 依赖树 | licensecheck / pipdeptree |
| R-33 | 无许可证×5（ark、llm_bidwriting、kcaaaa、biaoshuv2、RudyJunyu） | 默认版权保留，任何复制/改写分发均侵权 | 严重 | 仓库无 LICENSE 文件 | 目录检查 |
| R-34 | biaoshuv2 | 公开仓库含运达内部报价规则 + 国电投真实招标文件 | 严重 | 审计 §1/数据检查 | 抽样查看数据文件 |
| R-35 | RudyJunyu/Bid_Agent | .chroma 15MB（2,460 向量）+ 19MB 业务库误提交 | 严重 | git 对象大小 | `git count-objects -vH` |
| R-36 | ai_bidding_power_grid | 仓库内含真实电力行业招标数据集（外部版权） | 高 | 数据集目录 | README/数据文件抽样 |
| R-37 | llm_bidwriting | git 历史含真实业务数据（合同/客户名） | 中 | git 历史抽查 | `git log -p` 抽样 |

### 1.5 供应链与依赖

| ID | 仓库 | 风险 | 严重度 | 证据 | 复现/验证方法 |
|----|------|------|--------|------|--------------|
| R-38 | tender-cli | `base.py:81` 注解引用 try 块内导入的 `MCPServer`（openai-agents 未声明于 pyproject）→ 缺包 import 必抛 NameError [复核] | 高 | base.py 源码 | 不装 openai-agents 直接 `import tender_cli.mcp_tools.base` |
| R-39 | BidMaster-Pro | FastMCP 依赖错配 → MCP server 静默退化为 Stub（不报错） | 高 | 依赖/初始化路径 | 启动 MCP server 看工具表 |
| R-40 | ark-ai-bid-agent | `pdf-parse@1.1.1` 内嵌旧版 pdf.js，恶意 PDF 解析面 | 中 | package.json 锁定版本 | 依赖审计 |
| R-41 | mag05270/BidAgents | README 声称 LightRAG 但无 `import lightrag`（技术栈虚标样本） | 中 | 全库搜索 | `rg lightrag` |

## 2. 跨仓库共性失败模式（对本项目的警示）

| ID | 失败模式 | 出现仓库数 | 根因 | 我们的对策（落点文档） |
|----|---------|-----------|------|----------------------|
| S-1 | 截断/静默丢条款 | 8/15 | 无完整性对账 | 分块清单对账 + 否决项 100% 召回门（test-plan.md §T1/T2） |
| S-2 | 门禁可绕过（前端门/无门/fail-open） | 7/15 | 门禁不在服务端权威路径 | BLOCKED 状态由后端状态机强制 + waiver 审计（bid-mcp-contract.md §错误语义） |
| S-3 | README/许可证与实现不符 | ≥6/15 | 缺乏工程诚信基线 | 复用白名单 + clean-room 记录制度（本文件 §3 P4） |
| S-4 | 恢复假/默认关闭 | 3/15 | 状态不在权威存储 | 事件溯源 + resume 默认开启 + 恢复矩阵测试（test-plan.md §T5） |
| S-5 | 业务数据/二进制进 git | 4/15 | .gitignore 缺失 | 仓库准入清单 + pre-commit 扫描（mvp-implementation-plan.md M0） |
| S-6 | 死码/断链（功能声称存在但零调用） | 3/15 | 无契约/集成测试 | 每个业务工具一个契约测试，不依赖 LLM 可跑（test-plan.md §T8） |
| S-7 | LLM 失败静默降级（模板文本/空结果继续流转） | 4/15 | 失败语义未建模 | 工具错误码三分类：safe-retry / needs-input / permanent（bid-mcp-contract.md §4） |
| S-8 | 换模型/补遗无失效传播 | 15/15 | 无 requirement→section 依赖追踪 | 失效传播矩阵 + invalidation 重执行（bid-domain-model.md §5） |

## 3. 本项目设计与实施风险（前推）

| ID | 风险 | 严重度 | 缓解设计 | 验证方法 | 责任文档 |
|----|------|--------|---------|---------|---------|
| P-1 | 条款抽取召回不足导致废标 | 严重 | 六类分类 + 双通道互验 + 覆盖率对账 + 否决项硬门 | 合成语料召回测试（否决项召回必须 100% 才可过门） | test-plan.md T1/T2 |
| P-2 | 证据幻觉（无证据断言/虚构业绩） | 严重 | 证据先行原则、生成后引用回查、无证据→BLOCKED 而非生成 | 引用回查测试 + 缺证据用例必须阻断 | bid-skill-design.md §规则 2/3 |
| P-3 | 复用代码引入 AGPL/无许可证污染 | 严重 | 复用白名单（MIT/Apache 限定）+ clean-room 记录 + 依赖扫描 | CI license-check + 复用清单评审 | 本文件 §1.4 + repository-matrix §6 |
| P-4 | 状态机与实际流程漂移 | 高 | 状态转换集中定义 + 事件溯源 + 转换矩阵单元测试 | 全转换路径覆盖测试 | bid-domain-model.md §6 |
| P-5 | MCP 工具面过大/过细导致低能力模型误用 | 高 | 12 个工具上限 + 每工具一段描述 + 状态前置校验拒绝乱序调用 | 低能力模型（mock/小模型）工具选择测试 | bid-mcp-contract.md §7 |
| P-6 | 文档内核接口不稳定造成返工 | 中 | 适配器 + 最小契约（8 个操作）+ 版本协商字段 | 契约测试（stub 内核先行） | bid-domain-model.md §7 |
| P-7 | 暗标/签章/金额等确定性规则缺失 | 高 | 规则注册表 + 规则级测试（每规则一个用例） | 规则清单对账（全部规则有测试） | test-plan.md T6 |
| P-8 | 合成测试语料代表性不足 | 中 | 按真实招标文件结构建模 + 三行业模板 + 注入式变异 | 语料审查清单 + 每 release 跑回归 | test-plan.md §2 |
| P-9 | Skill 与工具能力失配（Get00 病：文档说 A 脚本是 B） | 高 | Skill 内仅引用工具名，参数 schema 由 MCP server 下发 | Skill 演练测试（dry-run 模式） | bid-skill-design.md §9 |
| P-10 | Token 成本失控（长文全量重塞上下文） | 中 | 阶段化上下文构造（五域上下文借鉴）+ 成本日志 | 每章 token 预算断言 | test-plan.md T9 |
