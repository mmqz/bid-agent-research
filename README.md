# AI 标书系统 · 开源调研与 Skill+MCP 架构设计（研究阶段产出）

> 阶段定位：**只研究、审计、设计**——未接入/修改文档内核，未实现完整业务，未提前锁定框架。
> 审计基线：18 个仓库（5 指定 + 8 搜索补充 + 5 框架参考），全部经 GitHub API 实时取证 + 源码抽检，原始证据见 `research/raw/`。

## 交付物索引

| 文档 | 内容 | 核心结论 |
|---|---|---|
| [research/repository-matrix.md](research/repository-matrix.md) | 18 仓库对比矩阵：许可证/活跃度/实现状态/复用判定 | MIT·Apache 可复用 7 家；AGPL 3 家禁抄；**BidMaster-Pro LICENSE=AGPL 但 README 写 MIT（冲突警报）** |
| [research/workflow-audit.md](research/workflow-audit.md) | 各仓库**真实执行链路**（✅/⚠️/❌ 节点级标注）+ 十项重点审计问题逐项结论 | 真实 Dify DSL / FastMCP Server / 业务 SKILL.md 均存在实证；无一家完整解决条款防丢 |
| [research/risk-register.md](research/risk-register.md) | 15 项风险：严重度/缓解/验证方法映射 | 六项 BLOCKER 级风险全部以**确定性代码**兜底，不依赖模型自觉 |
| [research/test-plan.md](research/test-plan.md) | 合成招标文件 25 埋点 + 12 用例矩阵 + MVP 实施顺序（M1~M6） | 条款召回/精确匹配/引用真实率设 100% 硬阈值 |
| [docs/bid-domain-model.md](docs/bid-domain-model.md) | 领域对象（PRJ/TF/REQ/EV/CMP/SEC/VF/RT）+ 状态机 + ID 命名空间隔离 | 条款/证据/文档块 ID 三套命名空间永不混用 |
| [docs/bid-mcp-contract.md](docs/bid-mcp-contract.md) | 8+1 个 MCP 工具契约（参数/返回/错误/幂等/状态转换）+ **KBAdapter 知识库适配器**（含 Dify API 端点核验与局限、Cherry 否决记录） | 知识库=import/search 的可替换实现层；自建为默认，Dify API 仅作 A/B 对比 |
| [docs/bid-skill-design.md](docs/bid-skill-design.md) | 主 Skill 九阶段路径（顺序锁死）+ 停止条件 + 恢复协议 + 反模式清单 | 封闭 Agent 只做编排核对，解析切块归后端确定性代码 |
| [docs/architecture-decision.md](docs/architecture-decision.md) | AD-01~AD-06 决策记录（含路径 A/B 对比矩阵、Dify KB API 选型三方案对比） | 不上完整 Dify；路径 A（Skill+MCP+状态机）；MVP 不引入图编排框架 |

## 架构一句话总结

**Skill 定义路径、MCP 暴露能力、后端持有状态**：封闭 Agent 按阶段推进并核对数字；8+1 个 MCP 工具薄壳化，业务逻辑与证据链在自有领域库；RAG 知识库做成可替换 Adapter（自建默认 / Dify API 可选 / Cherry 否决）；文档内核经最小稳定接口隔离访问，块 ID 与条款/证据 ID 隔离。

## 审计脚本

`scripts/gh_audit.py`（元数据+文件树+README 取证）、`scripts/gh_search.py`（补充仓库搜索）、`scripts/gh_deep.py`（关键源码抽检）。Token 经环境变量注入，不入库。
