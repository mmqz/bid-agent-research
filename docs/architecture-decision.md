# 架构决策记录（Architecture Decision）

> 决策编号 AD-01~AD-06。每条含：背景→决策→理由→被否方案→复议条件。全部基于 18 个仓库的实证审计（research/repository-matrix.md、research/workflow-audit.md）。

## AD-01 不采用完整 Dify 平台作为编排层

**决策**：不部署 Dify workflow/Chatflow 承载标书流程；标书业务不依赖其 DSL。仅可选使用其知识库 API 作为对比适配器（见 AD-03）。

**理由（实证）**：
1. **DSL 死板**：Dify workflow 适合固定单链路；标书流程是证据状态驱动的多阶段可重入流程（补遗文件到达→增量抽取→受影响章节 DIRTY 重跑），DSL 表达不了条件回退与影响面分析。
2. **状态被平台持有**：llm_bidwriting 实证——后端退化为 dify_client.py 粘合层（uploadFile→run_parse_workflow→stream_generate_chapter→run_check_workflow），业务状态一半在 Dify 平台库，**取不干净**（R-12）。
3. **版本绑死**：其 DSL schema 0.5.0 与现行 Dify 大版本存在迁移鸿沟；每次升级是持续成本（llm_bidwriting 的 3 个 YAML 即实证）。
4. **重量**：自托管栈为全家桶（api/worker/postgres/redis/向量库/sandbox/nginx 多容器），与"轻量可维护"目标冲突。

**复议条件**：出现强需求"非技术人员可视化编排单节点任务"时，可把单点原子能力（如某次抽取）外包给 Dify，但业务状态机与流程编排不迁移。

## AD-02 编排采用路径 A：Skill 驱动封闭 Agent + 后端状态机

**决策**：路径 A（SKILL.md 定义路径 + 8~9 个 MCP 工具 + 后端状态机/领域库）优于路径 B（Agent 自由规划 / LangGraph 图编排）。MVP 直接按路径 A 实施。

**对比矩阵**：

| 维度 | 路径 A：Skill+MCP+状态机 | 路径 B：自由规划/图编排（LangGraph 型） |
|---|---|---|
| 低能力 Agent 兼容 | ✅ 路径写死，门禁在后端（E_STATE 拒越级） | ❌ 依赖模型规划力，弱模型跑偏（tender-master"顺序锁死"的存在即证明业界踩过此坑） |
| 可恢复性 | ✅ 状态在自家库，断点/影响面分析任意实现 | ⚠️ 依赖框架 checkpointer（BidMaster 文件式 checkpoint 仅有 latest，无影响面）；LangGraph 有 checkpointer 但状态模型受框架约束 |
| 可测试性 | ✅ MCP 工具契约测试 + 状态机单测 + Skill evals（ark/LingYan 均有 evals 先例） | ⚠️ 图行为测试要 mock 节点集合，回归面大 |
| Token 成本 | ✅ 封闭路径→按阶段加载 references（渐进披露），长文件靠 offset/limit（LingYan 禁整读纪律） | ❌ 自由规划上下文易膨胀重试 |
| 维护成本 | ✅ 状态机表+Skill 文档化，人均可维护 | ⚠️ 图结构演化需懂框架；bidcraft 12 agent 图是维护反例风险 |
| 实证存活度 | LingYan / tender-master / claude-bid-gen 三个活跃实现（MIT） | bidcraft（Apache-2.0）单点存在，LUV803 宣称未深扫（UNVERIFIED） |

**结论**：路径 B 元素（图编排）仅在"多 Agent 并行起草+复杂回退"需求出现时局部引入（bidcraft 可作那时对照），MVP 不需要（见 AD-04）。

## AD-03 RAG 知识库选型：自建薄服务为默认，Dify API 为可选适配器，Cherry Studio 否决

**背景**（用户问）：不部署完整 Dify（太重），但需要它的 RAG 知识库能力；Skill 驱动封闭 Agent 完成文件处理→入库→检索；Cherry Studio 备选；"不方便自建的话用 Dify API 怎么样"。

**决策**：知识库做成 `KBAdapter` 接口（ingest/search 两个方法），**AdapterA 自建薄服务为默认实现**；**AdapterB = Dify Knowledge API 仅作为检索质量 A/B 对比与过渡期兜底**（锁版本+契约测试）；**Cherry Studio 知识库否决**。MCP 契约中知识库不是独立工具，而是 `import_tender_files` / `search_evidence` 的可替换实现层（docs/bid-mcp-contract.md 附录 A）。

**理由**：

| 维度 | A 自建（PGVector/Qdrant+FTS+确定性切块） | B Dify KB API（自托管） | B' Dify Cloud | C Cherry Studio |
|---|---|---|---|---|
| 部署重量 | 一个服务+一个向量库（或 PGVector 零新增） | **全家桶容器栈**（重量回来了） | 零部署 | 桌面应用（无服务端） |
| 数据边界 | ✅ 私有 | ✅ 私有 | ❌ **标书/资质出域，投标业务一票否决（R-09）** | 本地（但仅限单人桌面） |
| 分块控制 | ✅ 按条款编号结构切块，chunk_id 稳定，证据链可回溯 | ⚠️ 归 Dify：只能靠 create-by-text 逐段注入锚点硬拼 | 同左 | 客户端黑盒 |
| 精确匹配通道 | ✅ FTS/结构化索引等值查询（编号/型号/金额/日期） | ⚠️ hybrid 可配关键词权重，但无确定性精确保证，混合权重黑盒 | 同左 | — |
| 版本漂移 | 自控 | ⚠️ 端点命名/参数多次变更（create_by_file→create-by-file），15MB 文件上限需拆分 | 跟随云版本 | — |
| 许可证 | 自有 | 修改版 Apache2.0（多租户/品牌条款，对外 SaaS 需商业授权） | SaaS 条款 | **AGPL-3.0**（复用其代码传染） |
| 业界实证 | ark（pgvector+bigram）、rag-tender（FTS5+向量）均为 MIT 可复用 | llm_bidwriting 绑死 0.5.0 | — | 第三方 api-for-cherrystudio 另建 API 反证其客户端定位 |

**关键原则**：**解析和切块是后端确定性代码，Agent 只做编排**（调导入工具、核对统计数字、重试失败件）。"条款因分块遗漏丢失"（R-01）这类最重的风险必须由代码兜底，不能靠模型自觉。

## AD-04 MVP 不引入 LangGraph/图编排框架

状态机用"枚举+转换表+乐观锁"实现于后端（bid-domain-model.md §2），不引框架。理由：转换表 ~10 个状态、回退规则 1 条（DIRTY 重跑），LangGraph 的价值（复杂图/并行/人机中断）在此规模下是负资产；换框架成本 = 迁移全部状态语义。**复议条件**：多 Agent 并行起草（>3 个并发写作 Agent）或跨阶段复杂回退图出现时重评（对照 bidcraft）。

## AD-05 文档内核独立、接口最小、ID 隔离

- 不修改/不替换现有文档内核；经**稳定接口**访问：`create_block / update_block / get_block / render / export`（+ style_profile 概念）。
- 文档块 ID（DOC-*）由内核签发，业务侧只存 `content_ref` 引用；REQ/EV/SEC 与 DOC 三个命名空间**永不混用、不做映射改写**（用户要求）。
- 导出产物经 sha256 校验回传；页数统计以 Word 渲染口径为准（claude-bid-gen 实证：LibreOffice 页数不可信）。

## AD-06 可测试与可回滚

1. 每个 MCP 工具一条契约测试（含幂等重放）；状态机转换全覆盖单测。
2. 合成招标文件埋点回归集（research/test-plan.md）：条款完整性、精确匹配、断点恢复、审校命中四类是发布门禁。
3. KBAdapter 可切换 → 检索层 A/B（TC-10）不锁定供应商；模型层沿用 biaoshu 的 LiteLLM 统一入口思想（BYOK、可换模型）。
4. 复用代码白名单仅限 MIT/Apache 仓库（LingYan、rag-tender、tender-master、claude-bid-gen、bidcraft、biaoshu、rejection-check）；AGPL/无许可仓库只读设计不抄代码（R-10）。
