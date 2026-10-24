#!/usr/bin/env python3
"""run_tests.py — 每轮测试入口（test-plan §3 冒烟口径 + 夹具自检）

每轮开发/文档变更后运行：python3 scripts/run_tests.py
  T1 xref_check 常规模式（跨文档一致性，0 错误）
  T2 xref_check --inject（历史错误注入回归：R-11/R-14/TC-08 三处必须被捕获）
  T3 夹具完整性：manifest sha256 逐文件校验 + 结构计数（25 埋点/6+5+4+5+3+2 分布/13 证据/40×8/20×6）
  T4 埋点读回：25 条 GT 的 excerpt 逐字存在于对应源文件；B2 无文本层；B1/B3/C 关键串在位
  T5 判分器自测（完美运行全绿 + 7 类破坏注入全捕获）
  T6 确定性：重生成到临时目录，33 文件 sha256 全一致
退出码：0 全绿 / 1 存在失败（CI 门禁用）
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fixture_spec as S  # noqa: E402
import v1_corpus_r as R  # noqa: E402

REPO = Path(__file__).resolve().parent.parent
FIXTURES = REPO / "fixtures"
CORPUS = FIXTURES / "corpus-A"
# 期望文件数从规格派生（R2 P2-2：不再双处硬编码）
#   5 主文件 + 13 证据 + 4 GT + 12 转换 + 2 查询集 + 2 缺陷草稿 + 1 corpus-R README + 5 corpus-R 结构层
EXPECTED_MANIFEST_FILES = (5 + len(S.EVIDENCE_FILES) + 4 + len(S.TRANSITIONS) + 2 + 2 + 1
                           + len(R.OUTPUT_FILES))
_RULE_CODES = ["CERT_EXPIRED", "PRICE_EXCEEDS_BUDGET", "DURATION_MISMATCH", "PAGE_LIMIT",
               "MISSING_SIGNATURE", "DARK_BID_WORD", "TODO_RESIDUE", "INJECTION_PATTERN"]
_ECHO_RE = None  # 延迟编译（防 import 顺序）


def _echo_regex():
    """标记回声正则（R2 P0-6）：大小写不敏感 + 全角数字/连字符变体，匹配 GT-编号。"""
    global _ECHO_RE
    import re
    if _ECHO_RE is None:
        _ECHO_RE = re.compile(r"(?i)\s?gt\s?[-\u2010-\u2015\uFF0D]?\s?[0-9\uFF10-\uFF19]")
    return _ECHO_RE


_passed, _failed = [], []


def check(name: str, ok: bool, detail: str = ""):
    mark = "✓" if ok else "✗"
    print(f"  {mark} {name}" + (f" —— {detail}" if detail and not ok else (f"（{detail}）" if detail else "")))
    (_passed if ok else _failed).append(name)


def run(cmd: list[str]) -> tuple[int, str]:
    p = subprocess.run([sys.executable, *cmd], capture_output=True, text=True, cwd=REPO)
    return p.returncode, (p.stdout + p.stderr)


def fail_detail(out: str) -> str:
    """R2 P2-1：失败时展示全部 ✗ 行，不再只取末行。"""
    lines = [l.strip() for l in out.splitlines() if l.strip().startswith("✗")]
    return " | ".join(lines[:6]) if lines else (out.strip().splitlines()[-1] if out.strip() else "无输出")


# ---------------------------------------------------------------- T1/T2
def t_xref():
    print("[T1] xref_check 常规模式")
    code, out = run(["scripts/xref_check.py"])
    check("T1 跨文档一致性 0 错误", code == 0, fail_detail(out) if code else "")
    print("[T2] xref_check 注入自测")
    code, out = run(["scripts/xref_check.py", "--inject"])
    check("T2 历史错误注入全部捕获", code == 0, fail_detail(out) if code else "")


# ---------------------------------------------------------------- T3/T4
def _docx_text(path: Path) -> str:
    from docx import Document
    doc = Document(str(path))
    parts = [p.text for p in doc.paragraphs]
    for t in doc.tables:
        for row in t.rows:
            for cell in row.cells:
                parts.append(cell.text)
    return "\n".join(parts)


def _pdf_text(path: Path) -> str:
    import pdfplumber
    out = []
    with pdfplumber.open(str(path)) as pdf:
        for page in pdf.pages:
            out.append(page.extract_text() or "")
    return "\n".join(out)


def _norm_cn(s: str) -> str:
    """OCR 噪声容忍归一化：NFKC（全角→半角）+ 去空白 + 去标点。仅用于 GT↔OCR 对齐，不改变判分纪律。"""
    import re
    import unicodedata
    s = unicodedata.normalize("NFKC", s)
    return re.sub(r"[\s\W]+", "", s, flags=re.UNICODE)


def _b2_ocr_roundtrip(path: Path) -> str:
    """B2 扫描件 OCR 回读三段验证：像素层有字 → tesseract chi_sim 回读 → 归一化条款包含 + 四要素。
    返回 'ok' 或失败原因（环境缺 tesseract/chi_sim 属环境契约缺失，判 FAIL 不静默跳过）。"""
    try:
        import fitz  # PyMuPDF
        import numpy as np
        from PIL import Image
    except ImportError as e:
        return f"环境缺依赖：{e}"
    tess = None
    for cand in ("/usr/bin/tesseract", "/bin/tesseract"):
        import os
        if os.path.exists(cand):
            tess = cand
            break
    if not tess:
        return "环境契约缺失：tesseract 不可用"
    tessdata = Path.home() / ".tessdata"
    if not (tessdata / "chi_sim.traineddata").exists():
        return "环境契约缺失：chi_sim 语言数据不可用（~/.tessdata）"
    doc = fitz.open(str(path))
    imgs = doc[0].get_images(full=True)
    if not imgs:
        return "B2 无嵌入图像"
    pix = fitz.Pixmap(doc, imgs[0][0])
    img = Image.frombytes("L" if pix.n == 1 else "RGB", (pix.width, pix.height), pix.samples) \
        if pix.n <= 3 else None
    if img is None:  # CMYK 等罕见模式 → 走中转
        pix.save("/tmp/_b2_ocr.png")
        img = Image.open("/tmp/_b2_ocr.png")
    a = np.array(img.convert("L"))
    if int(a.min()) > 60 or (a < 150).sum() < 20000:
        return f"像素层无有效文字（min={a.min()}, dark={(a < 150).sum()}）——字体渲染失败回归"
    img.save("/tmp/_b2_ocr_input.png")
    env = {**__import__("os").environ, "TESSDATA_PREFIX": str(tessdata)}
    r = subprocess.run([tess, "/tmp/_b2_ocr_input.png", "stdout", "-l", "chi_sim", "--psm", "6"],
                       capture_output=True, text=True, env=env, timeout=120)
    if r.returncode != 0:
        return f"tesseract 失败：{r.stderr.strip()[:80]}"
    n_ocr = _norm_cn(r.stdout)
    clause = _norm_cn(S.B2_KEY_CLAUSE)
    if clause not in n_ocr:
        return "OCR 未回读出关键条款（归一化包含失败）"
    missing = [kw for kw in ("投标函", "投标总报价", "盖单位章", "日期") if _norm_cn(kw) not in n_ocr]
    if missing:
        return f"OCR 缺要素：{missing}"
    return "ok"


def t_fixture_integrity():
    print("[T3] 夹具完整性（manifest + 结构计数）")
    manifest = json.loads((CORPUS / "manifest.json").read_text(encoding="utf-8"))
    bad = []
    import hashlib
    for f in manifest["files"]:
        p = FIXTURES / f["path"]  # manifest 路径相对 fixtures 根
        if not p.exists():
            bad.append(f"{f['path']} 缺失")
        elif hashlib.sha256(p.read_bytes()).hexdigest() != f["sha256"]:
            bad.append(f"{f['path']} 哈希不符")
    # R2 P1-4：反向校验——磁盘→manifest（游离文件会被真实流水线当语料摄入）
    on_disk = {str(p.relative_to(FIXTURES)) for p in FIXTURES.rglob("*")
               if p.is_file() and p.name != "manifest.json" and not p.name.endswith(".tmp")}
    stray = sorted(on_disk - {f["path"] for f in manifest["files"]})
    if stray:
        bad.append(f"游离文件未入清单：{stray[:3]}")
    check(f"T3a manifest 双向校验（{EXPECTED_MANIFEST_FILES} 文件）",
          not bad and len(manifest["files"]) == EXPECTED_MANIFEST_FILES, "; ".join(bad[:3]))
    c = manifest["counts"]
    check("T3b 埋点 25 条且类别分布 6/5/4/5/3/2",
          c["requirements"] == 25 and c["categories"] == {"资格条件": 6, "否决项": 5, "评分项": 4,
                                                           "技术参数": 5, "商务要求": 3, "格式要求": 2},
          json.dumps(c["categories"], ensure_ascii=False))
    check("T3c 证据 13 件 + 冲突 1 对 + 转换 12 个 + 查询 25 条 + 章节 6 类 + 缺陷 8 类",
          c["evidence_files"] == 13 and c["conflicts"] == 1 and c["transitions"] == 12
          and c["exact_queries"] == 25 and c["sections"] == 6 and c["defects"] == 8)
    check("T3d 关键口径（开标 11-15→11-27；控制价 480→465 万；EV-02 至 11-25）",
          manifest["key_facts"]["open_bid_date"] == "2026-11-15 09:30"
          and manifest["key_facts"]["open_bid_date_after_addendum"] == "2026-11-27 09:30"
          and manifest["key_facts"]["control_price_a"] == 4_800_000
          and manifest["key_facts"]["control_price_c"] == 4_650_000)
    check("T3e B1 单价上限合计 ≤ 设备预算 350 万（无隐性冲突）",
          manifest["key_facts"]["b1_total_cap"] == S.B1_TOTAL_CAP and S.B1_TOTAL_CAP <= 3_500_000,
          f"合计 {S.B1_TOTAL_CAP:,} 元")


def t_readback():
    print("[T4] 埋点读回（ground truth ↔ 源文件逐字核对）+ 反标记回声")
    files = CORPUS / "files"
    a_text = _docx_text(files / "A-招标文件.docx")
    c_text = _docx_text(files / "C-补遗文件01.docx")
    b1_text = _pdf_text(files / "B1-设备清单.pdf")
    b2_text = _pdf_text(files / "B2-投标函格式页-扫描件.pdf")
    from openpyxl import load_workbook
    wb = load_workbook(str(files / "B3-图纸目录.xlsx"))
    ws = wb.active
    b3_text = "\n".join(str(cell.value) for row in ws.iter_rows() for cell in row if cell.value)

    # T4a：逐埋点逐锚点读回（B2 为图像页，按 B2_PAGE_TEXT 规格核对——OCR 真实性由 D5 矩阵/D5 管道承接）
    sources = {"A-招标文件.docx": a_text, "B1-设备清单.pdf": b1_text,
               "B2-投标函格式页-扫描件.pdf": S.B2_PAGE_TEXT, "C-补遗文件01.docx": c_text}
    miss = []
    for r in S.REQUIREMENTS:
        for a in r["anchors"]:
            src = sources.get(a["file"])
            if src is None:
                miss.append(f"{r['gt_id']} 无读回源（{a['file']}）")
            elif a["excerpt"] not in src:
                miss.append(f"{r['gt_id']}『{a['excerpt'][:18]}…』未见于 {a['file']}")
    check("T4a 25 埋点全部锚点逐字存在（含 GT-07×2 / GT-20×2 双锚点）", not miss, "; ".join(miss[:3]))

    dd_text = _docx_text(CORPUS / "defect-drafts" / S.DEFECT_DRAFT_NAME)

    # T4h（审计 S1 + R2 P0-6）：全语料反标记回声——A/C/B1/B3/B2 规格/证据/缺陷草稿全扫；
    # 大小写不敏感 + 全角数字/连字符变体 + 8 个规则码字面量
    echo = []
    corpus_texts = {"A.docx": a_text, "C.docx": c_text, "B1.pdf": b1_text, "B3.xlsx": b3_text,
                    "B2 规格": S.B2_PAGE_TEXT, "缺陷草稿": dd_text}
    for label, text in corpus_texts.items():
        if _echo_regex().search(text):
            echo.append(f"{label} 含 GT 标记")
        hits = [rc for rc in _RULE_CODES if rc in text]
        if hits:
            echo.append(f"{label} 含规则码 {hits[:2]}")
    ev_leak = [f.name for f in (files / "evidence").glob("*.pdf")
               if _echo_regex().search(_pdf_text(f)) or any(rc in _pdf_text(f) for rc in _RULE_CODES)]
    if ev_leak:
        echo.append(f"证据件泄露标记：{ev_leak[:2]}")
    check("T4h 反标记回声：全语料零 GT/规则标记（大小写/全角变体）", not echo, "; ".join(echo))

    check("T4b B1 设备清单含三型号 + 中文大写日期 + 40 行",
          all(m in b1_text for m in [S.MODELS["server"], S.MODELS["switch"], S.MODELS["firewall_old"]])
          and S.DELIVERY_CN in b1_text and b1_text.count("含安装辅材") >= 13)
    check("T4c B2 无文本层（OCR 路径输入特征）+ GT-07 键条款在页面规格中",
          b2_text.strip() == "" and S.B2_KEY_CLAUSE in S.B2_PAGE_TEXT, f"提取到 {len(b2_text.strip())} 字符")
    # T4c-OCR（v0.4）：扫描件 OCR 通道真实性——嵌入图像须有像素文字 + tesseract 回读关键条款与四要素。
    # 背景：v0.4 前可变字体静默渲染空白（图像只有公章没有字），OCR 通道建立在空图像上未被任何测试发现。
    ocr_note = _b2_ocr_roundtrip(files / "B2-投标函格式页-扫描件.pdf")
    check("T4c-OCR B2 扫描件 OCR 回读（像素层有字 + 条款归一化包含 + 四要素）",
          ocr_note == "ok", ocr_note if ocr_note != "ok" else "OCR 通道真实可用")
    check("T4d B3 图纸目录 GC-01~GC-20 在位", all(f"GC-{i:02d}" in b3_text for i in range(1, 21)))
    check("T4e 补遗 C 三变更在位（新型号/新开标日/新控制价）",
          S.MODELS["firewall_new"] in c_text and "2026-11-27 09:30" in c_text and "人民币 465 万元（含税）" in c_text)

    # T4g：TC-05 缺陷草稿读回——8 类缺陷样本逐字在位
    dm = json.loads((CORPUS / "defect-drafts" / "defect-map.json").read_text(encoding="utf-8"))
    dmiss = [d["rule_code"] for d in dm["defects"] if d["excerpt"] not in dd_text]
    check("T4g 缺陷草稿 8 类缺陷样本逐字在位", len(dm["defects"]) == 8 and not dmiss, str(dmiss))

    ev_dir = files / "evidence"
    ev_ok, ev_note = True, ""
    ev2 = _pdf_text(ev_dir / "EV-02-电子与智能化工程专业承包二级资质证书.pdf")
    ev13 = _pdf_text(ev_dir / "EV-13-安全生产许可证.pdf")
    if "2026 年 11 月 25 日" not in ev2:
        ev_ok, ev_note = False, "EV-02 有效期 2026-11-25 未见于证据"
    if "2025 年 08 月 01 日" not in ev13:
        ev_ok, ev_note = False, "EV-13 过期日 2025-08-01 未见于证据"
    check("T4f 证据陷阱在位（EV-02 临界 11-25；EV-13 过期 2025-08-01）", ev_ok, ev_note)


# ---------------------------------------------------------------- T5/T6
def t_grade():
    print("[T5] 判分器自测")
    code, out = run(["scripts/grade.py", "--self-test"])
    check("T5 场景全绿 + 13 类破坏全捕获（含 4 类 R2 攻击样本）", code == 0,
          fail_detail(out) if code else out.strip().splitlines()[-1] if out else "")


def t_determinism():
    print("[T6] 夹具确定性（重生成哈希比对）")
    with tempfile.TemporaryDirectory() as td:
        code, out = run(["scripts/fixture_gen.py", "--out", td])
        if code != 0:
            check("T6 生成器可重跑", False, out[-200:])
            return
        m1 = json.loads((CORPUS / "manifest.json").read_text(encoding="utf-8"))
        m2 = json.loads((Path(td) / "corpus-A" / "manifest.json").read_text(encoding="utf-8"))
        f1 = {f["path"]: f["sha256"] for f in m1["files"]}
        f2 = {f["path"]: f["sha256"] for f in m2["files"]}
        diff = [p for p in f1 if f1[p] != f2.get(p)]
        check(f"T6 {EXPECTED_MANIFEST_FILES} 文件重生成 sha256 全一致",
              not diff and len(f1) == len(f2) == EXPECTED_MANIFEST_FILES, "; ".join(diff[:3]))


# ---------------------------------------------------------------- T7（V1 四段验证）
def t_v1():
    """V1 全链路四段验证（verification-plan §3.1）：解析覆盖→引用回填→导出保真→暗标口径。
    共享一个临时目录：明标件构建一次供 T7b/c/d/e 复用（LO 渲染 3 次，约 15s）。"""
    print("[T7] V1 四段验证（解析覆盖 / 引用回填 / 导出保真 / 暗标口径）")
    import v1_citation
    import v1_darkbid
    import v1_export
    import v1_pipeline

    # T7a 解析覆盖
    rep_a = v1_pipeline.build_parse_report()
    ca, cr = rep_a["corpus_a"], rep_a["corpus_r"]
    check("T7a corpus-A 文本层锚点 26/26 + 扫描层登记 1（B2 OCR 路径）",
          ca["anchors"]["located"] == 26 and ca["anchors"]["text_layer_total"] == 26
          and ca["anchors"]["scan_deferred"] == 1 and not ca["anchors"]["failures"],
          "; ".join(f"{f['gt_id']}:{f['reason']}" for f in ca["anchors"]["failures"][:3]))
    check("T7a corpus-R 结构探针 11/11 + G7 归因检出 5/5",
          cr["locatable"]["pass"] == 11 and cr["locatable"]["expect"] == 11
          and cr["gap_attributed"]["detected"] == 5,
          "; ".join(f"{p['probe']}" for p in cr["probes"] if p["status"] == "not_found"))

    with tempfile.TemporaryDirectory() as td:
        td = Path(td)
        bright = td / "技术标-导出样本.docx"
        dark = td / "技术标-暗标样本.docx"
        info = v1_export.build_bid_docx(bright, "bright", workdir=td)
        v1_export.build_bid_docx(dark, "dark", workdir=td)

        # T7b 引用回填
        rep_b = v1_citation.run_citation_checks(bright, td)
        check("T7b 引用链 10 条回查全 valid（位置+指纹两级）", rep_b["R1_recheck"]["pass"])
        check("T7b 内容改写恰致 1 链 stale_content（其余 valid，只降不升）",
              rep_b["R2_content_stale"]["pass"])
        check("T7b 结构漂移致索引型定位器全体失效 + 书签重验恢复",
              rep_b["R3_structure_drift"]["pass"] and rep_b["R3b_bookmark_rebind"]["pass"])

        # T7c 导出保真
        rep_c = v1_export.run_fidelity_checks(bright, td / "lo_c")
        st, rd = rep_c["structure"], rep_c["render"]
        check("T7c 结构读回（书签10/域/图片/分页7/合并表/双分节/盖章位）",
              all(st.values()), "; ".join(k for k, v in st.items() if not v))
        check(f"T7c LO 渲染（{rd['pages']} 页：分页纪律/页眉页脚/图片在技术方案页/引用可见）",
              all(v for k, v in rd.items() if k != "pages"),
              "; ".join(k for k, v in rd.items() if k != "pages" and not v))

        # T7d 暗标口径
        rep_d = v1_darkbid.run_darkbid_checks(dark, bright, td)
        check(f"T7d 暗标四断言（页数 {rep_d['D1_pages']['pages']}≤上限且明暗一致 / 禁字双面零命中 / "
              f"元数据清零 / 金丝雀检出 {len(rep_d['D4_canary']['hits'])} 处防空洞绿）",
              rep_d["pass"],
              json.dumps({k: rep_d[k] for k in ("D2_forbidden_scan", "D3_metadata_zeroing")},
                         ensure_ascii=False)[:150] if not rep_d["pass"] else "")

        # T7e 导出件确定性（freeze_docx 字节级）
        again = td / "again.docx"
        info2 = v1_export.build_bid_docx(again, "bright", workdir=td)
        check("T7e 明标导出件重生成 sha256 一致（确定性）",
              info["docx_sha256"] == info2["docx_sha256"])


# ---------------------------------------------------------------- T8（V2 影响面验证）
def t_v2():
    """V2 补遗/答疑/版本影响面计算（verification-plan §3.2）：TC-09 契约消费 +
    级联完整性 + 无关不重跑 + 矛盾终裁门禁 + 幂等 + 删除/新增规则（影响面规则为
    bid-domain-model §2 设计语义的 Python 复刻 = SEMUL 口径，交叉验证待 cargo）。"""
    print("[T8] V2 影响面验证（TC-09 主场景 / 级联 / 无关不重跑 / 终裁门禁 / 幂等 / 规则覆盖）")
    import v2_impact
    report = v2_impact.run_v2()
    r = report["results"]
    check("T8 TC-09 断言契约全消费（expected-transitions 六断言）",
          r["R0_tc09_contract"]["pass"])
    check("T8 主场景 DIRTY 恰三章节 + GT-18 v2 + 冲突双版本登记 + 版本戳恰 +1",
          r["R1_tc09_main"]["pass"])
    check("T8 开标日级联：全项目重检 + GT-02 翻转 EXPIRED + EV-13 维持禁绑 + 零误翻",
          r["R2_cascade"]["pass"])
    check("T8 无关不重跑：售后/团队/格式三章零误伤",
          r["R3_no_overreach"]["pass"])
    check("T8 矛盾终裁门禁：脏阻断→未裁决阻断→终裁后放行 + 口径 465",
          r["R4_adjudication_gate"]["pass"])
    check("T8 同 sha 重复导入 no-op（幂等）", r["R5_idempotent"]["pass"])
    check("T8 删除/新增规则（SYNTHETIC）：绑定解除+行移除恰一章节 / 新增零误伤",
          r["R6_delete_add_rules"]["pass"])


# ---------------------------------------------------------------- T9（V3 内核集成三问）
def t_v3():
    """V3 文档内核集成边界（verification-plan §3.3）：三问断言（设计语义 SEMUL——
    doc-engine-audit §5 映射契约；引擎接入后升 ENGINE_RUN）。"""
    print("[T9] V3 内核集成三问（重写引用保持 / 撤销只降不升 / 导出中断一致性）")
    import v3_integration
    with tempfile.TemporaryDirectory() as td:
        report = v3_integration.run_v3(Path(td))
    r = report["results"]
    check("T9 Q1 五轮重写：每轮恰一链降级 + 重验恢复 + 无悬空书签",
          r["Q1_rewrite_5rounds"]["pass"])
    check("T9 Q2 撤销只降不升（undo 不自发恢复）+ 外部修改不静默 adopt",
          r["Q2_undo_and_reopen"]["pass"])
    check("T9 Q3 两阶段导出：中断停 EXPORTING 零假 EXPORTED + 恢复幂等同 sha",
          r["Q3_export_interrupt"]["pass"])


def main() -> int:
    print("=" * 62)
    print("ai-bid-research 每轮测试（V0 夹具 + 一致性 + 判分器 + V1/V2/V3 验证）")
    print("=" * 62)
    t_xref()
    t_fixture_integrity()
    t_readback()
    t_grade()
    t_determinism()
    t_v1()
    t_v2()
    t_v3()
    print("-" * 62)
    print(f"结果：{len(_passed)} 通过 / {len(_failed)} 失败")
    if _failed:
        print("失败项：", _failed)
    return 0 if not _failed else 1


if __name__ == "__main__":
    sys.exit(main())
