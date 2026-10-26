# 安全威胁模型与人工审批边界（S1）

> 定位：verification-plan §5.4 交付物。范围 = 标书系统 MVP 的**后端强制**安全面——
> 前提立场：Agent/Skill/模型属**不可信执行层**，一切防线以后端（bidcore + 文档内核
> mdcore）强制为准；提示词约束只作纵深一环，永不作唯一防线。
>
> 口径：每威胁一行——面 / 引擎继承（mdcore 已有机制，源码锚点见 doc-engine-audit §7）/
> 业务补充（bidcore/MCP 层职责）/ 测试锚点。状态词典同 verification-plan §6。
>
> 威胁建模基线：STRIDE 变体按四组展开（文件敌意面 / 越权与外发 / 提示注入 / 数据泄露），
> 外加供应链组；**T-03/T-04 为本次建模发现的真实缺口（GAP 登记，非既有设计）**。

## 1. 信任边界与信任级别

| 层 | 信任级别 | 依据 |
|---|---|---|
| 招标文件 / 证据文件（用户上传） | **不可信输入** | 任意来源文件——内容一律数据化，永不执行 |
| Agent（Skill 驱动的 LLM 操作者） | **不可信执行** | 模型可被注入操纵——工具参数 schema 化、无 shell/无自由 SQL、审校态不可写 |
| MCP 工具层（bid-tools） | 半可信 | serde 编译期契约（schema 拒绝在类型层）+ 状态机门禁（E_STATE） |
| 后端核心（bidcore domain/parse/kb） | 可信 | 确定性代码、无网络面、Result 纪律无 panic 路径 |
| 文档内核（mdcore） | 可信 | 容器安全门/消毒链/限额/PathPolicy/ReviewPolicy（审计已核） |
| 人工操作者（human: 前缀） | 最高 | 审批动作唯一合法来源 |

## 2. 威胁清单

### 2.1 文件敌意面（上传件 = 不可信输入）

| ID | 威胁 | 面 | 引擎继承（mdcore） | 业务补充（bidcore/MCP） | 测试锚点 | 状态 |
|---|---|---|---|---|---|---|
| T-01 | 恶意 Office 宏/OLE 载体（docm/xlsm 伪装） | 上传面 | office-io 读面仅走数据结构解析（宏不执行）；容器门+消毒链（§2.7） | 上传类型白名单：仅 docx/xlsx/pdf（宏载体格式直接拒收）；失败件隔离禁引（R-01 缓解） | M0 上传检查单；解析器拒收测试（恶意扩展名） | DESIGN_ASSUMPTION（M0 落地） |
| T-02 | PDF JavaScript 动作/嵌入对象 | 上传面 | mdc-pdf ExtractText 仅提取文本（JS 不执行） | 同 T-01 白名单；poppler 提取路径无执行面 | 解析器单测 + 组件版本矩阵（test-plan §5.2） | VERIFIED_SOURCE（poppler 行为） |
| T-03 | **压缩包炸弹**（高压缩比 XLSX/DOCX 撑爆内存） | 解析面 | office-io XLSX 三门 + 稀疏炸弹闭环（fd 校准实测，§2.7） | **GAP（本次建模发现）**：bid-parse xlsx 走 calamine `worksheet_range` 全量入内存、docx 走流式但无解压总量上限——引擎三门在 office-io 侧，bid-parse 适配层未挂限额 | 待补：bid-parse 限额单测（构造高压缩比样本 → 拒绝或限量解析） | **GAP → 待补（P1）** |
| T-04 | **外部解析器挂死**（畸形 PDF 卡住 pdftoppm/tesseract） | 解析面 | —（引擎自有渲染线不依赖外部进程） | **GAP（本次建模发现）**：bid-parse pdf.rs 子进程 `cmd.output()` 无超时——挂死即 DoS（注：arg 数组调用无 shell，命令注入面不存在） | 待补：`Command::output()` 包 timeout（如 60s）+ 挂死样本单测 | **GAP → 待补（P1）** |
| T-05 | 超大文件资源耗尽 | 上传面 | 写面限额（MAX_PARAGRAPHS/RUNS；G11 容量匹配） | 上传大小上限（M0 检查单）+ 解析块数统计返回（import 返回 stats 供核对） | M0 检查单；import stats 契约 | DESIGN_ASSUMPTION |

### 2.2 越权与外发

| ID | 威胁 | 面 | 引擎继承 | 业务补充 | 测试锚点 | 状态 |
|---|---|---|---|---|---|---|
| T-06 | 跨项目/跨企业资料越权 | 领域库 | — | ID 命名空间隔离（PRJ/REQ/EV/CMP/SEC 永不混用，domain-model §2）+ 资料库按项目隔离的访问范围 | M2 权限用例（越权访问他项目 EV → 拒绝） | DESIGN_ASSUMPTION |
| T-07 | 任意路径读写 | 工具面 | PathPolicy 允许列表模式（§2.7） | MCP 工具参数只收库内 ID（serde schema 编译期拒绝自由路径串——bid-tools 类型化契约） | cargo test 类型化 schema 拒绝用例（已在 M1 自测门禁） | TESTED（M1 schema 拒绝） |
| T-08 | 审批绕过（AI 写审校态/自我 WAIVE） | 审批面 | ReviewPolicy `human:` 前缀强制——AI 操作者在引擎层不可写审核态；失效不过审（§2.7） | 双层门禁：MCP §8 precheck（BLOCKER 清零/显式 WAIVE 前置）+ 引擎 allow_approval_of_broken_linkage=false（§5.5）；终裁门禁 V2 R4 已验（终裁前不得 ACCEPTED） | run_tests T8 R4；引擎 security.rs 语义锁测试 | TESTED（V2 SEMUL + 引擎单测） |
| T-09 | 数据出域外发 | 部署面 | —（本地纪律：sidecar 本地） | 部署红线：自托管、无公网出口依赖；外发域名白名单（若有通知场景）；R-09 数据边界 | 部署检查单 | DESIGN_ASSUMPTION |

### 2.3 提示注入纵深

| ID | 威胁 | 面 | 引擎继承 | 业务补充 | 测试锚点 | 状态 |
|---|---|---|---|---|---|---|
| T-10 | 文档内容注入（招标文件内藏指令文本） | 解析→起草 | — | **结构性免疫（确定性层）**：bidcore 解析/抽取/影响面全部为确定性代码，无模型消费文档原文——注入文本在解析层只是数据；L3 残余：起草模型消费抽取结果时注入面存在 → 文件内容数据标记（domain-model）+ Skill 纪律（bid-skill-design 反模式） | TC-05 规则 8（注入指令 8 类命中）；已知模式扫描仅告警不作防线 | PARTIAL（确定性层免疫=TESTED；起草层 L3 残余→人工审校门禁） |
| T-11 | 工具参数注入（诱导 Agent 构造恶意参数） | 工具面 | — | 路径 A 封闭 Agent 只做编排核对；MCP 工具全部参数 schema 化；无 shell/无自由 SQL/无任意路径（与 T-07 同面） | bid-tools 类型化 schema 拒绝（cargo test）；TC-11 E_STATE 越级防护 | TESTED（M1） |
| T-12 | 证据库投毒（伪造证据文件） | 证据面 | 证据三态 + 登记时刻指纹（内容换即失效） | 证据来源登记（文件 sha256 入库）+ 引用键纪律（R-04：引用存在≠支持——人工复核 L3）+ 上传者身份审计 | TC-04 引用支持率人工协议（test-plan §5.3） | PARTIAL（机制 TESTED；支持率人工协议待 M3） |

### 2.4 日志与数据泄露

| ID | 威胁 | 面 | 引擎继承 | 业务补充 | 测试锚点 | 状态 |
|---|---|---|---|---|---|---|
| T-13 | 证据摘录/sidecar 经日志泄露 | 日志面 | sidecar 本地纪律（md+meta 不出机器） | 日志脱敏清单：证据摘录截断或哈希化；trace.jsonl 不含证据全文（M1 已按此实现——trace 仅工具调用摘要） | 部署检查单 + trace 抽查 | PARTIAL（M1 trace 已合规；全链路脱敏清单 M5） |
| T-14 | 导出件元数据泄露（暗标身份暴露→废标） | 导出面 | —（G3 引擎侧缺口仍开） | **业务侧缓解已验（V1-d）**：docProps 清零 + app.xml Company 移除 + 禁字双面扫描 + 金丝雀防空洞绿 | run_tests T7d（每轮回归） | TESTED（业务侧）；引擎侧 G3 P0 待排期 |
| T-15 | 依赖/镜像/模型出口投毒 | 供应链 | — | R-10 白名单 + NOTICES 式传递依赖闭包审查；镜像固定摘要；模型出口=本地部署默认 | M0 合规审查 | PENDING |

## 3. 人工审批边界（强制点矩阵）

> 原则：**审批动作的下沉与双层**——引擎层 ReviewPolicy `human:` 前缀（AI 结构性不可写）
> + 业务层 MCP §8 precheck（状态机门禁）。任一层独立成立，不依赖单点。

| 动作 | 引擎层门禁 | 业务层门禁 | 状态 |
|---|---|---|---|
| 矩阵 policy 确认（eligible/not_eligible 终判） | Review `human:` | MCP §6 强制绑定 + 复核位 | DESIGN_ASSUMPTION（M2） |
| BLOCKER WAIVE | 失效不过审（allow_approval_of_broken_linkage=false） | MCP §8 WAIVE 记录（显式、留痕、可审计） | DESIGN_ASSUMPTION（M4） |
| 冲突终裁（CONFLICT 双版本裁决） | —（业务侧状态机） | V2 R4 已验：终裁前相关章节不得 ACCEPTED | TESTED（V2 SEMUL） |
| 最终导出放行 | Review `human:` | MCP §8：BLOCKER 清零/显式 WAIVE 前置 + sha256 落库为界 | TESTED（V3 Q3 两阶段一致性） |
| 提交（递交投标文件） | — | 人工动作——系统只准备不代交（bid-skill-design 停止条件） | DESIGN_ASSUMPTION |

## 4. 残余风险与待办

1. **T-03 压缩包炸弹限额（GAP，P1）**：bid-parse 适配层（calamine 全量内存 / zip 解压总量）未挂引擎三门——待补限额单测与实现（建议：文件解压后总量上限 + 单表行数上限，超限拒绝并登记失败件）。
2. **T-04 子进程超时（GAP，P1）**：`Command::output()` 包 timeout + 挂死样本回归。
3. **T-10 起草层注入（L3 残余，不可穷举）**：已知模式扫描仅告警；唯一收口 = 人工审校门禁（审校态 AI 不可写，T-08 双层门禁）。
4. **T-14 引擎侧 G3**：暗标元数据清零的业务侧缓解（zip 补丁）为过渡——引擎导出面产品化前，每次导出必须跑 T7d 断言（已入每轮回归）。
5. T-15 供应链闭包审查待 M0 执行（NOTICES 机制）。

> 以上 GAP 与待办已回填 risk-register（R-09/R-10/R-13 相关行注记）；T-03/T-04 建
> 议随 M1.5（解析器加固）一并处理——在真实用户文件接入前完成。
