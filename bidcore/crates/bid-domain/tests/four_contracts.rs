//! four_contracts.rs —— 四关键业务案例的 Rust 契约测试（P1-b）。
//!
//! Python SEMUL（V1-b/V2/V3）→ bidcore Rust 实装的交叉验证：
//!   C1 补遗级联失效（v2_impact.py R0~R5 断言组逐条对应）
//!   C2 证据关联只降不升（v1_citation R2 + v3 Q2：改写恰一链 / undo 不回升 /
//!      外部修改不自动 adopt）
//!   C3 多轮章节重写后锚点正确（v3 Q1：五轮重写、恰一链降级、零悬空、逐轮重验恢复）
//!   C4 导出中断不产生虚假成功（v3 Q3：停 EXPORTING、不变式、恢复幂等同 sha、
//!      与 ProjectStatus::Exported 状态机的一致性）
//!
//! 验证状态词典（verification-plan §6）：
//!   本文件全绿 = IMPACT/LINKAGE/EXPORT_GATE 语义 IMPLEMENTED；
//!   Python SEMUL 仍保留（文件级 DOCX 书签寻址证据）——双实现交叉对照，
//!   结果分记，互不折抵。

use bid_domain::export_gate::{ExportGate, ExportPhase};
use bid_domain::impact::{AcceptBlock, AddendumSpec, BindingStatus, ImpactEngine};
use bid_domain::linkage::{text_fingerprint, LinkLocator, LinkageLedger, LinkageStatus};
use bid_domain::ProjectStatus;
use std::collections::HashMap;

// ================================================================ C1 补遗级联失效

/// corpus-A C-补遗文件01 三变更联合（fixture_spec.py PROJECT/MODELS 同构值）。
fn corpus_a_addendum() -> AddendumSpec {
    AddendumSpec {
        new_model: "USG6705E-AC650".into(),     // MODELS.firewall_new
        new_eval_date: "2026-11-27".into(),     // PROJECT.open_bid_date_after_addendum
        control_price: 4_650_000,                // PROJECT.control_price_c
        conflict_gt_ids: vec!["GT-21".into(), "GT-08".into()],
    }
}

fn corpus_a_engine() -> ImpactEngine {
    let mut eng = ImpactEngine::corpus_a();
    eng.prime_validity(); // A 口径初始判定（GT-13 本已 EXPIRED——2025-08-01 < 2026-11-15）
    eng
}

#[test]
fn c1_addendum_cascade_dirty_exact_and_version_bump() {
    let mut eng = corpus_a_engine();
    let c_sha = "sha256:补遗C";
    let impact = eng.import_addendum("C-补遗文件01.docx", c_sha, &corpus_a_addendum());

    // R1：DIRTY 恰三章节（技术方案-网络与安全 / 商务部分-报价 / 资格响应）
    let mut dirty = impact.dirty_sections.clone();
    dirty.sort();
    assert_eq!(
        dirty,
        vec!["商务部分-报价", "技术方案-网络与安全", "资格响应"],
        "DIRTY 章节必须恰为三——多则影响面污染、少则级联漏网"
    );
    // R1：GT-18 v2 生效（latest_version_wins）
    let gt18 = eng.reqs().get("GT-18").unwrap();
    assert_eq!(gt18.params["current_model"].as_str().unwrap(), "USG6705E-AC650");
    // R1：CONFLICT-01 双版本登记
    let conflicts = eng.conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts["CONFLICT-01"].versions.len(), 2);
    assert!(!conflicts["CONFLICT-01"].resolved);
    // R1：content_version 恰 +1（一次导入一个版本戳——无新旧混合）
    assert_eq!(eng.content_version, 2);
    assert_eq!(eng.eval_date(), "2026-11-27");
}

#[test]
fn c1_cascade_precise_flips_no_overreach() {
    let mut eng = corpus_a_engine();
    let impact = eng.import_addendum("C-补遗文件01.docx", "sha-c", &corpus_a_addendum());

    // R2：GT-02 恰翻转 eligible → EXPIRED（2026-11-25 < 2026-11-27；资格判断下游）
    assert_eq!(eng.bindings()["GT-02"].status, BindingStatus::Expired);
    assert_eq!(eng.matrix()["GT-02"].policy, "not_eligible");
    assert!(impact.flips.contains(&"GT-02".to_string()));
    // R2：GT-13 维持 EXPIRED 禁绑（本已过期——重检不推翻；评分结论下游，细项 0 分）
    assert_eq!(eng.bindings()["GT-13"].status, BindingStatus::Expired);
    assert_eq!(eng.matrix()["GT-13"].score_subitem, Some(0));
    // R2：其余证据零误翻（重检 ≠ 全推翻）
    for gt in ["GT-01", "GT-05", "GT-06"] {
        assert_eq!(eng.bindings()[gt].status, BindingStatus::Valid, "{gt} 不得误翻");
    }
    // R2：重检日志恰两轮（A 口径 + C 口径）
    assert_eq!(eng.recheck_log, vec!["2026-11-15", "2026-11-27"]);

    // R3：无关不重跑——NOT DIRTY 恰为影响面外章节
    let mut clean: Vec<String> = eng
        .sections()
        .iter()
        .filter(|(_, s)| !s.dirty)
        .map(|(k, _)| k.clone())
        .collect();
    clean.sort();
    assert_eq!(clean, vec!["售后服务方案", "格式与签署", "项目团队"], "影响面外章节零误伤（恰三章节；UTF-8 码点序）");
}

#[test]
fn c1_adjudication_gate_and_idempotency() {
    let mut eng = corpus_a_engine();
    let c_sha = "sha-c";
    let _ = eng.import_addendum("C-补遗文件01.docx", c_sha, &corpus_a_addendum());

    // R4：脏阻断
    match eng.try_accept("商务部分-报价") {
        Err(AcceptBlock::Dirty) => {}
        other => panic!("终裁前脏章节不得 ACCEPTED，实际 {other:?}"),
    }
    // 重起草清脏 → 未裁决冲突仍阻断
    eng.redraft("商务部分-报价");
    match eng.try_accept("商务部分-报价") {
        Err(AcceptBlock::UnresolvedConflict(ids)) => {
            assert_eq!(ids, vec!["CONFLICT-01"]);
        }
        other => panic!("未裁决冲突必须阻断，实际 {other:?}"),
    }
    // 终裁（补遗优先 → 465 万）→ 放行 + 口径切换
    let (final_v, rule) = eng.adjudicate("CONFLICT-01").expect("终裁");
    assert_eq!(final_v, 4_650_000);
    assert!(rule.contains("补遗优先"));
    assert_eq!(eng.matrix()["GT-21"].control_price_caliber, Some(4_650_000));
    assert_eq!(eng.matrix()["GT-08"].control_price_caliber, Some(4_650_000));
    assert!(eng.try_accept("商务部分-报价").is_ok());

    // R5：幂等——同 sha 重复导入 no-op（version/DIRTY 均不动）
    let before_dirty = eng.dirty_sections();
    let again = eng.import_addendum("C-补遗文件01.docx", c_sha, &corpus_a_addendum());
    assert!(again.idempotent_noop);
    assert_eq!(eng.content_version, 2);
    assert_eq!(eng.dirty_sections(), before_dirty);
}

#[test]
fn c1_delete_add_rules_synthetic() {
    // R6（SYNTHETIC——D3 真实补遗链样本待建）：删除 → 绑定解除+行移除+恰一章节；
    // 新增（无章节归属）→ 新行 + 零章节误伤
    let mut eng = corpus_a_engine();
    let dirty = eng.clause_delete("GT-13");
    assert!(eng.reqs().get("GT-13").is_none());
    assert!(eng.bindings().get("GT-13").is_none());
    assert!(eng.matrix().get("GT-13").is_none());
    assert_eq!(dirty, vec!["售后服务方案"]);

    let dirty_add = eng.clause_add("SYN-NEW-01", "商务要求", "9.1", None);
    assert_eq!(eng.matrix()["SYN-NEW-01"].policy, "new_row");
    assert!(dirty_add.is_empty(), "无章节归属的新增零章节误伤");
}

// ================================================================ C2 证据关联只降不升

fn mk_locators(fps: &[&str]) -> HashMap<String, LinkLocator> {
    let mut m = HashMap::new();
    for (i, gt) in ["GT-01", "GT-02", "GT-05"].iter().enumerate() {
        m.insert(gt.to_string(), LinkLocator {
            paragraph_index: i,
            text_fingerprint: text_fingerprint(fps[i]),
        });
    }
    m
}

#[test]
fn c2_rewrite_degrades_exactly_one_chain_and_never_rises() {
    let locators = mk_locators(&[
        "见资质证书编号 D233012345。",
        "项目经理具有 PMP 证书。",
        "财务状况良好。",
    ]);
    let mut fingerprints: Vec<String> = vec![
        text_fingerprint("见资质证书编号 D233012345。"),
        text_fingerprint("项目经理具有 PMP 证书。"),
        text_fingerprint("财务状况良好。"),
    ];
    let chains: Vec<String> = locators.keys().cloned().collect();
    let mut ledger = LinkageLedger::new(&chains);

    // run 级改写：仅第 2 链所在段落被改 → 恰一链 stale_content（其余零误伤）
    fingerprints[1] = text_fingerprint("项目经理具有 PMP 证书（含培训记录）。");
    let degraded = ledger.refresh(true, &locators, &fingerprints);
    assert_eq!(degraded, vec!["GT-02"], "改写恰降一链——锚不跨链误伤");
    assert_eq!(ledger.status_of("GT-01"), Some(LinkageStatus::Valid));
    assert_eq!(ledger.status_of("GT-05"), Some(LinkageStatus::Valid));

    // undo：内容恢复原文 → Stale 不自发回升（只降不升）
    fingerprints[1] = text_fingerprint("项目经理具有 PMP 证书。");
    let re = ledger.refresh(true, &locators, &fingerprints);
    assert!(re.is_empty(), "恢复内容不产生新降级");
    assert_eq!(ledger.status_of("GT-02"), Some(LinkageStatus::Stale), "undo 后不得自发回升");

    // 显式重验（唯一回升通道）→ Valid
    assert!(ledger.reverify("GT-02", &locators["GT-02"], &fingerprints));
    assert!(ledger.all_valid());
}

#[test]
fn c2_external_change_no_silent_adopt() {
    let locators = mk_locators(&["a", "b", "c"]);
    let chains: Vec<String> = locators.keys().cloned().collect();
    let mut ledger = LinkageLedger::new(&chains);

    // 外部修改重开（adopt 面）：检测变化 → 不自动 adopt（Stale + 审计 + 转人工）
    ledger.detect_external_change("GT-01");
    assert_eq!(ledger.status_of("GT-01"), Some(LinkageStatus::Stale));
    assert!(!ledger.all_valid());
    // 审计有痕（OfferChoice → 业务默认转人工——doc-engine-audit §5.3 边界）
    assert!(matches!(
        ledger.audit.last(),
        Some(bid_domain::linkage::AuditEntry::ExternalChangeDetected { .. })
    ));
}

// ================================================================ C3 多轮重写后锚点正确

#[test]
fn c3_five_round_rewrite_anchors_stay_correct() {
    // 五轮重写循环：每轮恰改写一条引用段（其余链 valid）——
    // 语义等价于 V3 Q1 的 DOCX 文件循环（此处为指纹向量层；文件级书签证据在 SEMUL）
    let base_texts = [
        "投标函详见附件一。",
        "资质响应文件真实有效。",
        "技术方案满足第五章要求。",
        "项目经理到场承诺书见附件二。",
        "售后服务网点覆盖全国。",
    ];
    let chains: Vec<String> = (1..=5).map(|i| format!("CIT-{i}")).collect();
    let mut locators = HashMap::new();
    for (i, cid) in chains.iter().enumerate() {
        locators.insert(cid.clone(), LinkLocator {
            paragraph_index: i,
            text_fingerprint: text_fingerprint(base_texts[i]),
        });
    }
    let mut ledger = LinkageLedger::new(&chains);
    let mut fingerprints: Vec<String> = base_texts.iter().map(|t| text_fingerprint(t)).collect();

    for round in 0..5 {
        let victim = chains[round].clone();
        // 本轮改写 victim 段（追加润色文本）——内联捕获不支持数组索引，显式参数
        let rewritten = format!("{}（第 {} 轮润色）", base_texts[round], round + 1);
        fingerprints[round] = text_fingerprint(&rewritten);
        // 书签语义：victim 的新位置 = 原 index（同段改写不位移）——重验用新指纹
        let degraded = ledger.refresh(true, &locators, &fingerprints);
        // 恰一链降级（本轮改写者；此前轮次已显式重验恢复）
        assert_eq!(degraded, vec![victim.clone()], "第 {} 轮恰一链降级", round + 1);
        // 零悬空：全部链 ID 仍可解析（status 存在即定位器有效）
        for c in &chains {
            assert!(ledger.status_of(c).is_some(), "书签 {c} 不得悬空");
        }
        // 显式重验：rebind 到当前段落（同位置新指纹）→ 恢复 Valid
        let new_locator = LinkLocator {
            paragraph_index: round,
            text_fingerprint: fingerprints[round].clone(),
        };
        assert!(ledger.reverify(&victim, &new_locator, &fingerprints), "重验必须成功");
        assert!(ledger.all_valid(), "每轮重验后全链恢复——下一轮基线干净");
        locators.insert(victim, new_locator);
    }
    // 五轮后：全链 valid + 每轮审计有痕（降级 5 条 + 重验 5 条）
    assert!(ledger.all_valid());
    let degraded_events = ledger
        .audit
        .iter()
        .filter(|e| matches!(e, bid_domain::linkage::AuditEntry::BodyRefresh { .. }))
        .count();
    assert_eq!(degraded_events, 5);
}

// ================================================================ C4 导出中断不产生虚假成功

#[test]
fn c4_export_interrupt_never_fakes_exported() {
    // 中断注入：渲染后确认前"kill"（等效 V3 Q3 的 gate2.render 后不 confirm）
    let mut gate = ExportGate::new();
    let sha = gate.render(b"docx-artifact-bytes");
    assert_eq!(gate.state(), ExportPhase::Exporting, "中断后停 EXPORTING");
    assert!(!gate.is_exported(), "中断不得进入 EXPORTED");
    assert!(gate.artifact_sha().is_none(), "无落库就没有产物指纹——虚假成功条件封死");
    assert!(gate.consistent(), "不变式：EXPORTED ⟺ sha 落库");

    // 正常路径：confirm → EXPORTED + sha 落库
    assert!(gate.confirm());
    assert!(gate.is_exported());
    assert_eq!(gate.artifact_sha(), Some(sha.as_str()));
    assert!(gate.consistent());

    // 恢复路径（幂等）：重新确认同产物 → 同 sha
    assert!(gate.confirm());
    assert_eq!(gate.artifact_sha(), Some(sha.as_str()));

    // 独立会话重导同产物 → 同 sha（同产物幂等）
    let mut g2 = ExportGate::new();
    let sha2 = g2.render(b"docx-artifact-bytes");
    assert_eq!(sha, sha2, "同产物 sha 必须一致");
    g2.confirm();
    assert_eq!(g2.artifact_sha(), gate.artifact_sha());
}

#[test]
fn c4_exported_state_alignment_with_project_status() {
    // 导出门与 ProjectStatus 状态机的一致性：Exported 为终态且
    // ReadyToExport → Exported 是唯一入边（guard_tool 越级防护面）
    assert_eq!(ProjectStatus::Exported.next_allowed(), &[]);
    assert!(ProjectStatus::ReadyToExport.next_allowed().contains(&ProjectStatus::Exported));
    // 非法路径：Drafting 不得直接到 Exported
    assert!(!ProjectStatus::Drafting.next_allowed().contains(&ProjectStatus::Exported));
    // 治理失败（T-04 五分类）与导出门的语义闭环：Timeout/Overflow/NonZero
    // 均 Err → parse 不到 Ok → 不可能触发 confirm（guard 测试 timeout_never_lies_as_success
    // 已证）。此处断言状态机不接受"非 ReadyToExport"的导出意图。
    assert!(!ProjectStatus::FilesImported.next_allowed().contains(&ProjectStatus::Exported));
}
