# 标书业务 MCP 工具契约（Bid MCP Contract）

> 定位：MCP Server 是**薄壳**——工具只做参数校验、状态机裁决与转发，业务逻辑在后端服务层（参照 BidMaster services/mcp/server.py 的形态，但规避其 AGPL 实现）。每个工具均要求幂等；响应统一携带 `project_version`（乐观并发）。
> 通用响应包装：`{ ok, data, request_id, project_version?, warnings? }`；错误：`{ ok:false, error:{ code, message, hint? , item_errors? } }`。

## 0. 通用约定

- **错误码**：`E_AUTH` / `E_NOTFOUND` / `E_STATE`（状态机拒绝越级或非法转换）/ `E_CONFLICT`（project_version 过期）/ `E_PARTIAL`（部分条目失败，附 item_errors）/ `E_UNPROCESSABLE`（解析不可恢复）/ `E_RATE` / `E_INTERNAL`。
- **幂等策略**：写操作接受 `idempotency_key`；同一 key 重复调用返回首次结果（不重复执行）。文件按 `sha256` 天然幂等；条款抽取按 `dedupe_hash` 幂等合并。
- **补充建议（第 9 个只读工具）**：`get_bid_project_state`——断点恢复的入口（读项目状态、各阶段完成度、DIRTY 章节、BLOCKER 计数），仿 LingYan `check_pipeline_state`。原 8 工具契约不受影响。

## 1. create_bid_project
```
in : { name, tender_notice_ref?, bid_deadline, mode(明标|暗标), external_key }
out: { project_id, status: "DRAFT" }
幂等: external_key 相同 → 返回已有 project_id（E_PARTIAL 不适用，静默幂等）
错误: E_CONFLICT（deadline 已过 → 创建即警告 warning=BID_DEADLINE_PAST）
状态: → DRAFT
```

## 2. import_tender_files（内含知识库入库）
```
in : { project_id, files:[{upload_ref, filename, kind(招标文件|附件|图纸|补遗|答疑|评分办法|格式模板)?, expected_pages?}], project_version }
out: { file_ids:[TF-*], per_file:[{file_id, sha256, pages, tables, chunks, ocr_tier, parse_status, errors?}],
       totals:{files, chunks, parse_failed, partial} }
幂等: sha256 已存在 → no-op 返回原 file_id（deduplicated:true）
语义: 后端确定性管线——解析（表格单元格级、扫描件 OCR 分档）→ 结构化切块（chunk_id 稳定、登记 文件→位置 映射）→ 证据注册 → 向量+精确双索引。**Agent 不做解析与切块，只核对返回统计并重试失败件（R-01）。**
错误: E_PARTIAL（部分文件 parse FAILED；失败件内容全程禁引）；E_STATE（project 已过 IMPORTED 阶段追加新文件 → 允许，但标 dirty 波及）
状态: DRAFT → FILES_IMPORTED
```

## 3. extract_requirements
```
in : { project_id, scope:"all"|file_ids[], options:{granularity, include_scoring_table:true}, project_version }
out: { counts:{total, by_category{qualification,veto,scoring,tech_param,commercial,format,other}},
       conflicts:[{a:REQ-*,b:REQ-*,reason}], missing_sequence:[{file_id,expected_clause_nos,found}], warnings[] }
执行: 确定性先行（条款编号序列解析、表格结构化槽位抽取）→ 模型补全语义归类与陈述规范化；按 dedupe_hash 幂等合并重跑结果
硬规则: 条款编号缺号 → missing_sequence 必须报告（完整性校验）；补遗与正文冲突 → 双向登记 conflicts
错误: E_PARTIAL（某文件抽取失败，其余成功）；E_STATE（未导入文件）
状态: FILES_IMPORTED → REQUIREMENTS_EXTRACTED
```

## 4. get_compliance_matrix（只读）
```
in : { project_id, filters:{category?, response_policy?, gap?, status?}, page? }
out: { rows:[{cmp_id, requirement:{id, clause_no, category, statement, mandatory, source_anchor, params},
              evidence:[{ev_id, title, status, valid_until}], gap, response_policy, draft_section_id}], counts }
无状态副作用；任何阶段可调。
```

## 5. search_evidence（知识库检索——双通道）
```
in : { project_id, query, mode:"hybrid"|"semantic"|"exact", exact?:{field:"model|amount|date|clause_no|cert", value},
       filters:{source_type?, tags?, valid_at?:开标日, min_valid_until?}, top_k, threshold? }
out: { hits:[{ev_id, chunk_id, content, source:{file, position}, score, match_type:"semantic|exact|both"}],
       trace:{semantic_k, exact_k, merged_by} }
硬规则: 资格类查询默认 exact 优先；valid_until < 开标日 的证据自动降权并附 warning=EVIDENCE_EXPIRING/EXPIRED；
      排序带确定性 tie-breaker（score, evidence_id）保证可重放（R-15）
错误: E_STATE（项目无企业资料库绑定）
```

## 6. draft_response_section
```
in : { project_id, section_id? | spec:{title, order, parent_id?, toc_ref}, instructions,
       source_requirements:[REQ-*], evidence_ids:[EV-*], style_profile?, project_version }
out: { section_id, content_ref:"DOC-*", version, stats:{words, citations, unanchored_claims:0}, warnings[] }
幂等: (section_id, instructions_hash, inputs_version) 相同 → no-op 返回原稿
前置: 矩阵 READY（资格/否决行 policy 已填）；EV 状态 OK（EXPIRED 拒绝绑定→E_UNPROCESSABLE）；
     引用的 REQ 必须在 depends_on 中
执行: 后端经**文档内核稳定接口**创建/更新 DOC 块（本系统不触碰内核内部结构）；引用覆盖率校验——承诺句无证据引用计为 unanchored_claims>0 时 warning（阻断留给 validate）
状态: SEC → DRAFTED；项目 → DRAFTING
```

## 7. validate_bid_document
```
in : { project_id, scope:"all"|section_ids[], rules_profile:"standard|dark_bid", project_version }
out: { findings:[{vf_id, kind:DETERMINISTIC|MODEL, rule_code, severity, section_id?, requirement_id?, message, evidence}],
       counts:{blocker, major, minor, deterministic, model} }
规则分层（Q7）:
  确定性（代码断言，先行、必跑）: 报价 vs 控制价/预算、工期 vs 招标要求、资质有效期 vs 开标日、
      页数/字数上限（Word 渲染口径）、必填格式项、签章位存在性、暗标禁字清单扫描、条款引用完整性
  模型（仅产 finding 不改稿）: 表述质量、逻辑连贯、偏离合理化、疑似过度承诺
状态: → VALIDATING → REVIEWING；BLOCKER 清零（或登记 WAIVED+理由）才可 EXPORT
```

## 8. export_bid_document
```
in : { project_id, format:"docx"|"pdf", profile:{dark_bid? , template?}, options:{tracked_changes?, watermark?}, project_version }
out: { artifact:{url, sha256, size}, content_version_snapshot, precheck:{blockers:0, waived:[VF-*]} }
前置: 所有 BLOCKER findings = Resolved/Waived（否则 E_STATE + 阻断清单）
幂等: (project_id, content_version, format, profile) 相同 → 返回缓存 artifact
状态: → READY_TO_EXPORT → EXPORTED
```

---

## 附录 A：KBAdapter（知识库适配器接口）

> 对用户问题的落地：**知识库不是 MCP 工具，而是 import_tender_files / search_evidence 的可替换实现层**。接口先行，实现可切换。

```python
class KBAdapter(Protocol):
    def ingest(self, project_id, files: list[IngestFile]) -> IngestReport:  # chunks/失败清单
        ...
    def search(self, project_id, query, mode, filters, top_k) -> list[Hit]:  # 双通道命中
        ...
```

### AdapterA：自建薄服务（默认实现，推荐终态）
- 存储：PGVector 或 Qdrant（向量）+ FTS5/全文（精确）+ 对象存储（原文件）；SQLite FTS5 方案见 rag-tender（MIT 可复用）。
- 分块：**确定性结构化切块器**——按条款编号/标题层级切分，chunk_id 稳定并登记 `chunk → file → 位置`（证据链可回溯，ark/rag-tender 同路）。
- 检索：语义 + 精确双通道，元数据过滤（类型/行业/有效期），交替合并 + 确定性排序。

### AdapterB：Dify 知识库 API（可选对比实现，不作核心依赖）
官方文档核验过的端点（https://docs.dify.ai，Knowledge API key 鉴权）：

| 能力 | 端点 | 备注 |
|---|---|---|
| 建库 | `POST /datasets` | indexing_technique: high_quality/economy |
| 文本/文件入文档 | `POST /datasets/{id}/document/create-by-file`（旧版 `create_by_file`） | multipart：file + data JSON（process_rule 可配 separator/max_tokens/chunk_overlap） |
| 索引进度 | `GET /datasets/{id}/documents/{batch}/indexing-status` | batch 由创建返回 |
| 分段管理 | `POST/GET/DELETE /datasets/{id}/documents/{doc_id}/segments[/{seg_id}]` | 可注入证据锚点 |
| 检索 | `POST /datasets/{id}/retrieve`（retrieval_model: semantic/full_text/hybrid + 权重 + rerank + top_k/threshold） | hybrid 可配 keyword/vector 权重 |

**局限（引用自官方文档+实测社区证据）**：
1. 自托管版依附整套容器栈（api/worker/postgres/redis/向量库/sandbox/nginx）——"太重"未解决；Cloud 版零部署但**数据出域，投标业务不可接受（R-09）**。
2. 文件默认 15MB 上限（`UPLOAD_FILE_SIZE_LIMIT`），大招标文件需拆分上传。
3. 分块归 Dify 所有：chunk 边界与 ID 不受控，证据链须靠"逐段 create-by-text + 注入 `[EV-锚点]`"硬拼，弱化可回溯性（R-08）。
4. 版本漂移：端点命名与 retrieval_model 结构在 0.x→1.x 多次变更（旧文档 `create_by_file` vs 现行 `create-by-file`）；llm_bidwriting 的 DSL 绑死 0.5.0 是同型实证。
5. 许可证：Dify 开源许可证为修改版 Apache2.0（品牌与多租户 SaaS 条款），内部自用无碍，对外多租户 SaaS 需商业授权。

**结论**：AdapterB 仅用于检索质量 A/B 对比（TC-10）与过渡期兜底，锁版本、包一层契约测试。终态以 AdapterA 为默认。

### AdapterC：Cherry Studio 知识库（否决记录）
CherryHQ/cherry-studio（AGPL-3.0，52.9k★）为 Electron **桌面客户端**，知识库为本地功能（客户端解析+本地索引），无 headless 服务端 API；第三方 api-for-cherrystudio（141★）需另建 API 服务反证其客户端定位。**不能作为封闭 Agent 的后端 RAG**。其 MCP 客户端能力可用于人工调试前端，不作为运行时。
