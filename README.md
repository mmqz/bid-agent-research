# AI 标书系统 · 开源调研与 Skill+MCP 架构设计（研究阶段产出）

> 阶段定位：**只研究、审计、设计**——未实现完整业务，未提前锁定框架。v2 起进入**研究验证阶段**（见 research/verification-plan.md）。
> 审计基线：18 个仓库（5 指定 + 8 搜索补充 + 5 框架参考），全部经 GitHub API 实时取证 + 源码抽检，原始证据见 `research/raw/`；**文档引擎审计基线**：mmqz/mdcore（main=6daeace，只读）+ mmqz/cleanroom-analysis（main=149b32e），2026-10-23。

## 交付物索引

| 文档 | 内容 | 核心结论 |
|---|---|---|
| [research/repository-matrix.md](research/repository-matrix.md) | 18 仓库对比矩阵：许可证/活跃度/实现状态/复用判定 | MIT·Apache 可复用 7 家；AGPL 3 家禁抄；**BidMaster-Pro LICENSE=AGPL 但 README 写 MIT（冲突警报）** |
| [research/workflow-audit.md](research/workflow-audit.md) | 各仓库**真实执行链路**（✅/⚠️/❌ 节点级标注）+ 十项重点审计问题逐项结论 | 真实 Dify DSL / FastMCP Server / 业务 SKILL.md 均存在实证；无一家完整解决条款防丢 |
| [research/risk-register.md](research/risk-register.md) | 15 项风险 + **三层安全保证模型（L1 输入完整性/L2 规则正确性/L3 语义正确性）** + 全量追溯矩阵（风险→设计→模块→用例→残余风险） | v2 修正 3 处测试映射错误；"确定性兜底"收紧为"确定性校验≠确定性正确"，L3 残余归人工审校门禁 |
| [research/test-plan.md](research/test-plan.md) | 合成招标文件 25 埋点 + 12 用例矩阵 + **泛化与压力测试（真实结构分布测试集 D1~D6/组件版本矩阵/引用支持率人工协议/重跑稳定性/导出复验）** | 100% 阈值是**验收目标不是已证性能**；主夹具=第一套回归基准，泛化能力另验 |
| [research/verification-plan.md](research/verification-plan.md) | **验证阶段执行计划**：可执行评测夹具三件套、三条关键链路验证（V1 解析→导出/V2 补遗影响面/V3 内核集成）、最小架构对照实验 EXP-1（Skill+MCP+状态机 vs Dify 五场景）、六方向落地任务、验证状态词典与回填纪律 | 下一步不堆仓库——验证假设、补风险、小型原型证明可行 |
| [docs/bid-domain-model.md](docs/bid-domain-model.md) | 领域对象（PRJ/TF/REQ/EV/CMP/SEC/VF/RT）+ 状态机 + ID 命名空间隔离 | 条款/证据/文档块 ID 三套命名空间永不混用 |
| [docs/bid-mcp-contract.md](docs/bid-mcp-contract.md) | 8+1 个 MCP 工具契约（参数/返回/错误/幂等/状态转换）+ **KBAdapter 知识库适配器**（含 Dify API 端点核验与局限、Cherry 否决记录） | 知识库=import/search 的可替换实现层；自建为默认，Dify API 仅作 A/B 对比 |
| [docs/bid-skill-design.md](docs/bid-skill-design.md) | 主 Skill 九阶段路径（顺序锁死）+ 停止条件 + 恢复协议 + 反模式清单 | 封闭 Agent 只做编排核对，解析切块归后端确定性代码 |
| [docs/architecture-decision.md](docs/architecture-decision.md) | AD-01~AD-07 决策记录，**全部标注验证状态（TESTED/VERIFIED_SOURCE/DESIGN_ASSUMPTION/PENDING）** | 不上完整 Dify（论据已收紧，待 EXP-1 实测补证）；路径 A；AD-07=文档引擎对齐 mdcore 既有契约 |
| [docs/doc-engine-audit.md](docs/doc-engine-audit.md) | **文档引擎只读审计**（mdcore + cleanroom-analysis，源码锚点级）：块锚点/证据三态/八步闭环/ReviewPolicy/Office/PDF 能力面；差距矩阵 G1~G11；集成边界映射（两系统共同维护版本与证据关系）；引擎能力需求清单 P0/P1/P2 | 引擎成熟度高于预期，集成从"自建薄适配"改为"对齐既有契约"；P0 缺口=目录域/文档级格式/暗标元数据清零（需引擎团队排期） |
| [docs/mvp-implementation-plan.md](docs/mvp-implementation-plan.md) | MVP 实施顺序（M0~M5） | M3 起接真实内核（接口按 AD-07 修订对齐） |

## 架构一句话总结

**Skill 定义路径、MCP 暴露能力、后端持有状态**：封闭 Agent 按阶段推进并核对数字；8+1 个 MCP 工具薄壳化，业务逻辑与证据链在自有领域库；RAG 知识库做成可替换 Adapter（自建默认 / Dify API 可选 / Cherry 否决）；文档内核（mdcore）经其既有契约面（op/anchor/workbench 命令/wasm 出口）集成，块 ID 与条款/证据 ID 隔离，证据失效判定用指纹、审计用版本。

## 当前状态与下一步

- 已完成：18 仓库实证审计 + 全套设计文档（v1）→ **评审意见落实（v2）**：一致性修正、三层保证模型、泛化测试设计、引擎审计与 AD-07、验证状态标注。
- 进行中：研究验证阶段（verification-plan §7 顺序）——V0 夹具 → V1 解析导出 → EXP-1 对照 → V2/V3 → S1/S2/S3 → 决策是否进入 MVP。
- 引擎侧协同：P0 能力需求（目录域/文档级格式/暗标元数据清零）已列 doc-engine-audit §6，经用户转达引擎团队排期。

## 审计脚本

`scripts/gh_audit.py`（元数据+文件树+README 取证）、`scripts/gh_search.py`（补充仓库搜索）、`scripts/gh_deep.py`（关键源码抽检）。Token 经环境变量注入，不入库。
