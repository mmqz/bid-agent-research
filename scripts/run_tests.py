#!/usr/bin/env python3
"""run_tests.py — 每轮测试入口（test-plan §3 冒烟口径 + 夹具自检 + M1 场景判分）
每轮开发/文档变更后运行：python3 scripts/run_tests.py
  T1 xref_check 常规模式（跨文档一致性，0 错误）
  T2 xref_check --inject（历史错误注入回归：R-11/R-14/TC-08 三处必须被捕获）
  T3 夹具完整性：manifest sha256 逐文件校验 + 结构计数（25 埋点/6+5+4+5+3+2 分布/13 证据/40×8/20×6）
  T4 埋点读回：25 条 GT 的 excerpt 逐字存在于对应源文件；B2 无文本层；B1/B3/C 关键串在位
  T5 判分器自测（完美运行全绿 + 13 类破坏注入全捕获）
  T6 确定性：重生成到临时目录，39 文件 sha256 全一致
  T7 M1 Rust 工具链：cargo build --release + 5 项单测（状态机守卫/幂等/OCR 规整）
  T8~T11 M1 四场景判分（TC-01/02/07/11）：G1 召回 / G2 精确 / G5 场景断言 / G7 幂等全绿，
       失败维度集合必须恰为 {G3_policy, G4_citation, G6_rules}（M2/M3/M4 范围 pending——
       出现集合外的失败即回归）
  T9  V1-a 解析覆盖：M1 产出 blocks.jsonl 对回 27 个 GT 锚点逐字命中（corpus-A 100% 标准）
  T12 M1 确定性：同场景双跑 db/trace/blocks/chunks 四工件字节一致（含 OCR 通道）
退出码：0 全绿 / 1 存在失败（CI 门禁用）
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
import os
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fixture_spec as S  # noqa: E402

REPO = Path(__file__).resolve().parent.parent
FIXTURES = REPO / "fixtures"
CORPUS = FIXTURES / "corpus-A"
M1_DIR = REPO / "m1"
M1_BIN = M1_DIR / "target" / "release" / "m1"
M1_SCOPE = ("G1_recall", "G2_exact", "G5_transitions", "G7_idempotency")
M1_PENDING = {"G3_policy", "G4_citation", "G6_rules"}  # M2/M3/M4 范围——db 无该字段，判分器诚实 FAIL
SCENARIOS = ("TC-01", "TC-02", "TC-07", "TC-11")
# 期望文件数从规格派生（R2 P2-2：不再双处硬编码）
EXPECTED_MANIFEST_FILES = (5 + len(S.EVIDENCE_FILES) + 4 + len(S.TRANSITIONS) + 2 + 2 + 1)
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


# ---------------------------------------------------------------- T7~T12：M1 Rust 流水线
def _cargo() -> str | None:
    return shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo")


def _m1_run(scenario: str, out: Path) -> tuple[int, str]:
    p = subprocess.run([str(M1_BIN), "run", "--scenario", scenario,
                        "--corpus", str(CORPUS / "files"),
                        "--queries", str(FIXTURES / "queries" / "exact-25.jsonl"),
                        "--out", str(out)],
                       capture_output=True, text=True, cwd=REPO)
    return p.returncode, p.stdout + p.stderr


def _grade_dims(run_dir: Path) -> dict:
    """判分一次 M1 运行：返回 {维度: pass}（report.json 由 grade.py 写入）。"""
    subprocess.run([sys.executable, "scripts/grade.py", "--run", str(run_dir)],
                   capture_output=True, text=True, cwd=REPO)
    rep = json.loads((run_dir / "report.json").read_text(encoding="utf-8"))
    return {k: bool(v.get("pass")) for k, v in rep["dimensions"].items()}


def t_m1():
    print("[T7] M1 Rust 工具链（构建 + 单测）")
    cargo = _cargo()
    if not cargo:
        check("T7a cargo 可用（~/.cargo/bin）", False, "未找到 cargo 工具链")
        return
    b = subprocess.run([cargo, "build", "--release"], capture_output=True, text=True,
                       cwd=M1_DIR, env={**os.environ, "PATH": f"{Path.home()}/.cargo/bin:{os.environ['PATH']}"})
    ok_build = b.returncode == 0 and M1_BIN.exists()
    check("T7a m1 构建成功（cargo build --release）", ok_build,
          fail_detail(b.stdout + b.stderr) if not ok_build else "")
    t = subprocess.run([cargo, "test", "--release"], capture_output=True, text=True,
                       cwd=M1_DIR, env={**os.environ, "PATH": f"{Path.home()}/.cargo/bin:{os.environ['PATH']}"})
    n_ok = t.stdout.count("... ok") + t.stdout.count("test result: ok")
    check("T7b m1 单测全绿（状态机守卫/幂等/OCR 规整）", t.returncode == 0 and "0 failed" in t.stdout,
          fail_detail(t.stdout + t.stderr) if t.returncode else f"{n_ok} 项")
    if not ok_build:
        return

    runs = REPO / "runs"
    runs.mkdir(exist_ok=True)
    for sc in SCENARIOS:
        code, out = _m1_run(sc, runs / sc)
        if code != 0:
            check(f"T8 {sc} 场景运行（step expects 自检）", False, fail_detail(out))
            return
    check("T8a 四场景运行 step expects 自检零失败（TC-01/02/07/11）", True,
          "create→import×3→extract→chunk→search×5→replay→guard 全部通过")

    # 四场景判分：M1 范围四维全绿 + 失败集恰为 pending 三项
    for sc in SCENARIOS:
        dims = _grade_dims(runs / sc)
        bad = [d for d in M1_SCOPE if not dims.get(d)]
        unexpected = {d for d, v in dims.items() if not v} - M1_PENDING
        check(f"T8b {sc} 判分：G1/G2/G5/G7 全绿且无范围外失败",
              not bad and not unexpected,
              f"失败维度: {sorted((d for d, v in dims.items() if not v))}" if bad or unexpected else "")

    # T9：V1-a 解析覆盖（blocks.jsonl 对回 27 锚点——corpus-A 100% 标准）
    print("[T9] V1-a 解析覆盖（M1 实际产出 vs GT 锚点）")
    blocks: dict[str, list[str]] = {}
    for line in (runs / "TC-01" / "blocks.jsonl").read_text(encoding="utf-8").splitlines():
        blk = json.loads(line)
        blocks.setdefault(blk["file"], []).append(blk["text"])
    miss, total = [], 0
    for r in S.REQUIREMENTS:
        for a in r["anchors"]:
            total += 1
            if not any(a["excerpt"] in t for t in blocks.get(a["file"], [])):
                miss.append(f"{r['gt_id']}@{a['file']}")
    check(f"T9 锚点覆盖 {total - len(miss)}/{total}（corpus-A 100% 标准；含 GT-07×2 扫描件 OCR / GT-20×2 日期歧义）",
          not miss, "; ".join(miss[:3]))

    # T12：M1 确定性（双跑字节一致，含 OCR 通道）
    print("[T12] M1 确定性（同场景双跑四工件字节一致）")
    with tempfile.TemporaryDirectory() as td:
        d1, d2 = Path(td) / "r1", Path(td) / "r2"
        _m1_run("TC-01", d1)
        _m1_run("TC-01", d2)
        diff = [f for f in ("db.json", "trace.jsonl", "blocks.jsonl", "chunks.jsonl")
                if (d1 / f).read_bytes() != (d2 / f).read_bytes()]
        check("T12 双跑 db/trace/blocks/chunks 字节一致（OCR 通道含 OMP_THREAD_LIMIT=1 钉版）",
              not diff, "; ".join(diff))


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


def main() -> int:
    print("=" * 62)
    print("ai-bid-research 每轮测试（V0 夹具 + 一致性 + 判分器 + M1 场景）")
    print("=" * 62)
    t_xref()
    t_fixture_integrity()
    t_readback()
    t_grade()
    t_determinism()
    t_m1()
    print("-" * 62)
    print(f"结果：{len(_passed)} 通过 / {len(_failed)} 失败")
    if _failed:
        print("失败项：", _failed)
    return 0 if not _failed else 1


if __name__ == "__main__":
    sys.exit(main())
