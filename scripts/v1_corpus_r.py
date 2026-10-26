#!/usr/bin/env python3
"""v1_corpus_r.py — corpus-R 结构分布测试集生成器（V1-a 首批：D1/D2/D5 三层）

verification-plan §3.1 V1-a 要求 corpus-R D1-tables / D2-nonbody / D5-scanned 分层
跑解析面。真实件去标识化样本在 V0 阶段不可得（D3/D4/D6 仍待真实样本），本生成器
以**确定性合成**复刻三层的结构特征（合并表头/跨页表/公式区、页眉页脚/文本框/批注/
修订、无文本层/双栏/盖章遮挡/图文混排）——结构压力即测试目的，内容全为示例虚构。

纪律：
- 反标记回声（同 corpus-A）：语料正文零 GT/规则标记；
- 确定性：无随机、无时间戳；zip 冻结（ooxml_util.freeze_zip）；PDF invariant；
  PIL→PNG 内容寻址（ImageReader，不走文件路径）；
- 文件全部经 fixture_gen.main() 生成 → 自动入 manifest（run_tests T3a 双向校验）。
"""
from __future__ import annotations

import io
import sys
import zipfile
from pathlib import Path

import reportlab.rl_config
reportlab.rl_config.invariant = 1  # 确定性 PDF（须在 canvas 前设置）

from docx import Document
from docx.enum.section import WD_SECTION
from docx.enum.table import WD_TABLE_ALIGNMENT
from docx.oxml.ns import qn
from openpyxl import Workbook
from openpyxl.styles import Alignment, Font, PatternFill
from PIL import Image, ImageDraw, ImageFont
from reportlab.lib.pagesizes import A4
from reportlab.lib.units import mm
from reportlab.pdfgen import canvas as rl_canvas

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ooxml_util import XLSX_FIXED_CORE, freeze_zip, freeze_docx  # noqa: E402

NOTO_PATH = "/usr/share/fonts/truetype/chinese/SarasaMonoSC-Regular.ttf"
# 字体事实（v0.4 B2 修复同步）：NotoSansSC[wght].ttf 可变字体在 PIL 下渲染全空白
# （FreeType 变量轴实例化失败，0 暗像素）——D5 扫描层图像与导出件插图一律用静态
# 字体，且生成时做像素级自证断言（防“空扫描页”静默溜过）。


def _assert_pixels_text(img, min_dark: int, label: str):
    """像素自证：文字必须真实渲染在像素层（防可变字体静默空白）。"""
    import numpy as np
    a = np.asarray(img.convert("L"))
    dark = int((a < 150).sum())
    assert dark >= min_dark, f"{label} 暗像素过少（{dark}<{min_dark}）——文字未渲染（可变字体空白回归？）"
    return dark

CORPUS_R_VERSION = "v0.1（V1-a 首批 D1/D2/D5）"

# 产出文件清单（run_tests 期望文件数单一真源；gen_corpus_r 断言保持同步）
OUTPUT_FILES = (
    "D1-tables/D1a-合并表头跨页表.docx",
    "D1-tables/D1b-宽表公式.xlsx",
    "D2-nonbody/D2a-非正文条款.docx",
    "D5-scanned/D5a-全扫描双栏盖章.pdf",
    "D5-scanned/D5b-图文混排.pdf",
)

README = """# corpus-R：真实结构分布测试集

> v0.1（V1-a 首批）：D1-tables / D2-nonbody / D5-scanned 三结构层已建。
> 生成器：`scripts/v1_corpus_r.py`（确定性合成——复刻真实件的**结构特征**而非内容）。

## 已建分层

| 层 | 文件 | 结构特征 | 解析面验证口径 |
|---|---|---|---|
| D1-tables | D1a-合并表头跨页表.docx | 两行嵌套表头（横向合并×2 + 纵向跨行合并）、18 数据行长表（渲染自然跨页）、正文内合并单元格 | 表格单元格定位（DocxTableCell 行列编址在合并下稳定） |
| D1-tables | D1b-宽表公式.xlsx | 合并标题区、公式列（F=D×E）、合计 SUM、数据区纵向合并 | 公式双通道指纹（值变/公式变各自失效）+ 区域指纹行优先序 |
| D2-nonbody | D2a-非正文条款.docx | 页眉页脚载明限制条款、首页不同页眉、文本框条款（VML）、批注（comments part）、修订（w:ins/w:del） | 页眉页脚可读可定位；文本框/批注/修订**盘点但无定位器**（引擎缺口 G6，业务侧 M1 同边界——诚实上报不伪装支持） |
| D5-scanned | D5a-全扫描双栏盖章.pdf | 全页无文本层（图像字）、双栏排版、半透明印章遮挡 | 文本层检测=none → OCR 路径（M1 解析面外，登记不伪装） |
| D5-scanned | D5b-图文混排.pdf | 第 1 页文本层 + 第 2 页纯图像 | 文本层检测=partial（逐页判定，非全有全无） |

## 待建分层（需真实样本去标识化）

- D3-revisions/ 多版本修订链：补遗/答疑/澄清多次修改、相互矛盾、部分撤销
- D4-wording/ 表述漂移：同一要求不同表达、无固定编号、口语化、"参照执行"类引用
- D6-scale/ 超长超大：500 页级正文、附件图纸包、Excel 量价清单

来源纪律（真实样本入库时）：仅公开招标公告/文件去标识化构造——去除投标人/评审人
信息，保留版式与条款结构；入库登记来源 URL 与抓取日期。合成件（本 v0.1 批）无
任何真实主体信息，去标识化纪律自动满足。

完整分层定义：research/test-plan.md §5.1；验证执行：scripts/v1_pipeline.py（V1-a）。
"""

# ---------------------------------------------------------------- D1a DOCX 合并表头跨页表
_D1A_BODY = [
    # (类别[纵向合并起点], 名称, 型号规格, 单位, 数量, 单价上限, 质保期, 备注)
    ("网络设备", "核心交换机", "示例型号 CE-9500-48C（示例）", "台", 2, 185_000, "3 年", "含模块"),
    (None,      "接入交换机", "示例型号 CE-6800-48T（示例）", "台", 12, 23_500, "3 年", ""),
    (None,      "无线控制器", "示例型号 WLC-3504（示例）", "台", 2, 78_000, "3 年", ""),
    ("安全设备", "下一代防火墙", "示例型号 FG-3100D（示例）", "台", 2, 156_000, "3 年", "含特征库"),
    (None,      "上网行为管理", "示例型号 AC-6000（示例）", "台", 1, 45_000, "3 年", ""),
    ("计算存储", "应用服务器", "示例型号 SR-660（示例）", "台", 4, 92_000, "3 年", ""),
    (None,      "数据库服务器", "示例型号 SR-860（示例）", "台", 2, 148_000, "3 年", ""),
    (None,      "分布式存储", "示例型号 ST-5000（示例）", "套", 1, 320_000, "3 年", ""),
    ("机房配套", "精密空调", "示例型号 CRV-40（示例）", "台", 2, 68_000, "2 年", ""),
    (None,      "UPS 电源", "示例型号 UH-60kVA（示例）", "套", 1, 210_000, "2 年", ""),
    (None,      "机柜及配件", "示例型号 NR-42U（示例）", "个", 14, 4_800, "2 年", ""),
    ("智能化专项", "视频监控", "示例型号 IPC-8X（示例）", "台", 48, 3_200, "2 年", ""),
    (None,      "门禁系统", "示例型号 AC-2100（示例）", "套", 16, 5_600, "2 年", ""),
    (None,      "综合布线", "六类非屏蔽（示例）", "项", 1, 185_000, "2 年", ""),
    (None,      "排队叫号", "示例型号 QM-300（示例）", "套", 8, 12_000, "2 年", ""),
    ("运维服务", "驻场运维", "3 年 7×24（示例）", "项", 1, 260_000, "—", "含备件"),
    (None,      "培训服务", "分批集中培训（示例）", "项", 1, 36_000, "—", ""),
    (None,      "迁移实施", "数据迁移与割接（示例）", "项", 1, 55_000, "—", ""),
]


def gen_d1a(out: Path) -> dict:
    path = out / "D1a-合并表头跨页表.docx"
    doc = Document()
    doc.add_heading("附件一：设备与技术服务量价表（示例——合并表头结构测试件）", level=1)
    doc.add_paragraph("说明：本表为两行嵌套表头（横向合并 + 纵向跨行合并）结构示例；"
                      "表体 18 行，渲染后自然跨页；单价上限合计不超过示例预算。")
    # 9 列：序号 | 设备信息(名称/型号规格/单位) | 商务信息(数量/单价上限/质保期) | 类别 | 备注
    t = doc.add_table(rows=2 + len(_D1A_BODY), cols=9)
    t.style = "Table Grid"
    t.alignment = WD_TABLE_ALIGNMENT.CENTER
    t.cell(0, 0).merge(t.cell(1, 0))          # 序号 纵向合并
    t.cell(0, 1).merge(t.cell(0, 2)).merge(t.cell(0, 3))   # 设备信息 横向合并
    t.cell(0, 4).merge(t.cell(0, 5)).merge(t.cell(0, 6))   # 商务信息 横向合并
    t.cell(0, 7).merge(t.cell(1, 7))          # 类别 纵向合并
    t.cell(0, 8).merge(t.cell(1, 8))          # 备注 纵向合并
    t.cell(0, 0).text = "序号"
    t.cell(0, 1).text = "设备信息"
    t.cell(0, 4).text = "商务信息"
    t.cell(0, 7).text = "类别"
    t.cell(0, 8).text = "备注"
    for i, name in enumerate(["名称", "型号规格", "单位"]):
        t.cell(1, 1 + i).text = name
    for i, name in enumerate(["数量", "单价上限（元）", "质保期"]):
        t.cell(1, 4 + i).text = name
    # 表体：类别列纵向合并（连续同类行合并——合并后仅首行保留类别名）
    for r, (cat, name, spec, unit, qty, price, warranty, note) in enumerate(_D1A_BODY):
        row = 2 + r
        t.cell(row, 0).text = str(r + 1)
        t.cell(row, 1).text = name
        t.cell(row, 2).text = spec
        t.cell(row, 3).text = unit
        t.cell(row, 4).text = str(qty)
        t.cell(row, 5).text = f"{price:,}"
        t.cell(row, 6).text = warranty
        if cat:
            t.cell(row, 7).text = cat          # 分组首行写类别名，随后纵向合并
        t.cell(row, 8).text = note
    # 类别列合并：网络设备(0-2) 计算存储(5-7) 机房配套(8-10) 智能化专项(11-14) 运维服务(15-17)
    for start, end in [(0, 2), (5, 7), (8, 10), (11, 14), (15, 17)]:
        t.cell(2 + start, 7).merge(t.cell(2 + end, 7))
    doc.add_paragraph("注：本表结构为合成示例，不构成任何真实采购需求。")
    doc.save(str(path))
    freeze_docx(path)
    return {"path": path.name, "cols": 9, "body_rows": len(_D1A_BODY),
            "header": {"rows": 2, "horizontal_merges": 2, "vertical_merges": 2},
            "body_vertical_merges": [("类别列", 5)]}


# ---------------------------------------------------------------- D1b XLSX 宽表公式
_D1B_ROWS = [
    ("布线材料", "六类网线（示例）", "箱", 120, 620),
    (None, "光缆（示例）", "米", 2000, 18),
    (None, "配线架（示例）", "个", 24, 480),
    ("网络模块", "接入交换机（示例）", "台", 12, 23500),
    (None, "核心交换机（示例）", "台", 2, 185000),
    ("安全组件", "防火墙（示例）", "台", 2, 156000),
    (None, "行为管理（示例）", "台", 1, 45000),
    ("机房材料", "UPS 电池（示例）", "节", 80, 1250),
    (None, "空调冷媒（示例）", "kg", 60, 260),
    ("施工辅材", "桥架（示例）", "米", 850, 42),
    (None, "线管（示例）", "米", 1200, 12),
    (None, "辅材杂项（示例）", "项", 1, 38000),
]


def gen_d1b(out: Path) -> dict:
    import datetime
    path = out / "D1b-宽表公式.xlsx"
    wb = Workbook()
    wb.properties.created = wb.properties.modified = datetime.datetime(2026, 10, 16, 9, 0, 0)
    ws = wb.active
    ws.title = "量价汇总"
    ws["A1"] = "量价汇总表（示例——公式与合并区结构测试件）"
    ws["A1"].font = Font(bold=True, size=13)
    ws.merge_cells("A1:F1")
    headers = ["序号", "分类", "项目", "单位", "数量", "合价（元）"]
    ws.append(headers)
    for cell in ws[2]:
        cell.font = Font(bold=True)
        cell.fill = PatternFill("solid", fgColor="DDDDDD")
        cell.alignment = Alignment(horizontal="center", vertical="center")
    for i, (cat, item, unit, qty, unit_price) in enumerate(_D1B_ROWS):
        r = 3 + i
        ws.cell(r, 1, i + 1)
        ws.cell(r, 2, cat if cat else None)
        ws.cell(r, 3, item)
        ws.cell(r, 4, unit)
        ws.cell(r, 5, qty)
        ws.cell(r, 6, f"=E{r}*{unit_price}")   # 公式通道：合价 = 数量 × 单价
    # 分类列纵向合并（连续同类行：3-5 布线 / 6-7 网络 / 8-9 安全 / 10-11 机房 / 12-14 施工）
    for start, end in [(3, 5), (6, 7), (8, 9), (10, 11), (12, 14)]:
        ws.merge_cells(f"B{start}:B{end}")
    last = 2 + len(_D1B_ROWS)
    ws.cell(last + 1, 5, "合计")
    ws.cell(last + 1, 5).font = Font(bold=True)
    ws.cell(last + 1, 6, f"=SUM(F3:F{last})")
    ws.cell(last + 1, 6).font = Font(bold=True)
    for col, width in zip("ABCDEF", [6, 12, 22, 8, 8, 14]):
        ws.column_dimensions[col].width = width
    wb.save(str(path))
    freeze_zip(path, patches={"docProps/core.xml": XLSX_FIXED_CORE})
    formulas = [f"F{3 + i}" for i in range(len(_D1B_ROWS))] + [f"F{last + 1}"]
    return {"path": path.name, "rows": len(_D1B_ROWS), "formulas": formulas,
            "merged": ["A1:F1"]}


# ---------------------------------------------------------------- D2a DOCX 非正文条款
_D2A_HEADER_CLAUSE = "示例项目澄清文件（第 1 次）｜页眉载明：本页眉所示内容与正文具有同等效力"
_D2A_FOOTER_CLAUSE = "受控文件编号：CL-2026-SY-001（页脚载明：受控编号条款与正文具有同等效力）"
_D2A_TEXTBOX_CLAUSE = "文本框条款：本文件所附技术参数表以最新澄清版本为准（本条款载于文本框内，非正文段落）"
_D2A_COMMENT = "评审查疑点：本条与后续补遗可能存在口径差异（示例批注——批注不属正文）"
_D2A_INS_TEXT = "（修订插入：响应截止时间顺延至 2026-12-01 09:30）"
_D2A_DEL_TEXT = "原表述：响应截止时间 2026-11-20 09:30"


def _add_textbox_paragraph(doc, text: str):
    """VML 文本框段落（python-docx 正文段落遍历不可见其文本——非正文结构特征）。"""
    from docx.oxml import parse_xml
    ns = ('xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
          'xmlns:v="urn:schemas-microsoft-com:vml" '
          'xmlns:o="urn:schemas-microsoft-com:office:office"')
    xml = (
        f'<w:p {ns}><w:r><w:pict>'
        f'<v:shape id="tbClause1" type="#_x0000_t202" style="width:340pt;height:56pt;">'
        f'<v:textbox><w:txbxContent><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:txbxContent></v:textbox>'
        f'</v:shape></w:pict></w:r></w:p>'
    )
    doc.element.body.append(parse_xml(xml))


def _add_revision_paragraph(doc, base_text: str, ins_text: str, del_text: str):
    """修订段：w:ins 插入 + w:del 删除（python-docx p.text 对两者均不可见——修订特征）。"""
    from docx.oxml import parse_xml
    ns = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
    author, date = "示例作者", "2026-10-20T09:00:00Z"
    xml = (
        f'<w:p {ns}>'
        f'<w:r><w:t xml:space="preserve">{base_text}</w:t></w:r>'
        f'<w:ins w:id="901" w:author="{author}" w:date="{date}">'
        f'<w:r><w:t xml:space="preserve">{ins_text}</w:t></w:r></w:ins>'
        f'<w:del w:id="902" w:author="{author}" w:date="{date}">'
        f'<w:r><w:delText xml:space="preserve">{del_text}</w:delText></w:r></w:del>'
        f'</w:p>'
    )
    doc.element.body.append(parse_xml(xml))


def _add_comment(doc, paragraph, text: str):
    """comments part 注入（python-docx 1.2 无公开批注 API——按 OOXML 规约手写）。"""
    from docx.oxml import parse_xml
    W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    # 1) 在目标段落首 run 前插 commentRangeStart，末插 commentRangeEnd + commentReference
    first_run = paragraph.runs[0]._r
    rng_start = parse_xml(f'<w:commentRangeStart xmlns:w="{W}" w:id="701"/>')
    rng_end = parse_xml(f'<w:commentRangeEnd xmlns:w="{W}" w:id="701"/>')
    ref = parse_xml(f'<w:r xmlns:w="{W}"><w:rPr><w:rStyle w:val="a5"/></w:rPr>'
                    f'<w:commentReference w:id="701"/></w:r>')
    first_run.addprevious(rng_start)
    paragraph._p.append(rng_end)
    paragraph._p.append(ref)


def gen_d2a(out: Path) -> dict:
    path = out / "D2a-非正文条款.docx"
    doc = Document()
    doc.add_heading("示例项目澄清文件（第 1 次）", level=1)
    p1 = doc.add_paragraph("澄清一：机房建设标准按现行国家标准执行，联合接地电阻不大于 1 欧姆；"
                           "本条与招标文件第三章相应条款具有同等效力。")
    doc.add_paragraph("澄清二：投标文件递交地点调整为示例大厦三层开标室（示例地址）；"
                      "递交截止时间以本文件页眉及修订标记载明的口径为准。")
    doc.add_paragraph("澄清三：评分细则中『类似的类似业绩』认定年限，以合同签订时间在近三年内为准。")
    _add_textbox_paragraph(doc, _D2A_TEXTBOX_CLAUSE)
    _add_revision_paragraph(doc, "澄清四：响应截止时间口径如下——", _D2A_INS_TEXT, _D2A_DEL_TEXT)
    # 页眉页脚（默认 + 首页不同）
    sec = doc.sections[0]
    sec.different_first_page_header_footer = True
    sec.header.paragraphs[0].text = _D2A_HEADER_CLAUSE
    sec.footer.paragraphs[0].text = _D2A_FOOTER_CLAUSE
    first_hdr = sec.first_page_header
    first_hdr.paragraphs[0].text = "示例项目澄清文件（第 1 次）——首页（首页页眉与默认页眉不同）"
    _add_comment(doc, p1, _D2A_COMMENT)
    doc.save(str(path))
    # comments part zip 层注入
    W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    comments_xml = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f'<w:comments xmlns:w="{W}">'
        f'<w:comment w:id="701" w:author="示例作者" w:date="2026-10-20T09:00:00Z" w:initials="SY">'
        f'<w:p><w:r><w:t>{_D2A_COMMENT}</w:t></w:r></w:p></w:comment></w:comments>'
    ).encode("utf-8")
    with zipfile.ZipFile(path) as zin:
        items = {i.filename: zin.read(i.filename) for i in zin.infolist()}
        types = items["[Content_Types].xml"].decode("utf-8")
        if "comments+xml" not in types:
            types = types.replace(
                "</Types>",
                '<Override PartName="/word/comments.xml" ContentType='
                '"application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/></Types>')
            items["[Content_Types].xml"] = types.encode("utf-8")
        rels = items["word/_rels/document.xml.rels"].decode("utf-8")
        if "comments.xml" not in rels:
            rels = rels.replace(
                "</Relationships>",
                '<Relationship Id="rIdCmt701" '
                'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" '
                'Target="comments.xml"/></Relationships>')
            items["word/_rels/document.xml.rels"] = rels.encode("utf-8")
        items["word/comments.xml"] = comments_xml
    tmp = path.with_suffix(".docx.tmp")
    with zipfile.ZipFile(tmp, "w", compression=zipfile.ZIP_DEFLATED) as zout:
        for name, data in items.items():
            zout.writestr(name, data)
    tmp.replace(path)
    freeze_docx(path)
    return {"path": path.name,
            "header_clause": True, "footer_clause": True, "first_page_header_diff": True,
            "textbox": 1, "comments": 1, "revisions": {"ins": 1, "del": 1}}


# ---------------------------------------------------------------- D5 扫描层
_D5A_LEFT = ["评分因素：技术方案完整性", "评分因素：设备参数响应程度", "评分因素：类似业绩",
             "评分因素：项目团队配置", "评分因素：售后服务承诺", "评分因素：实施进度计划"]
_D5A_RIGHT = ["分值：20 分", "分值：15 分", "分值：10 分", "分值：10 分", "分值：15 分", "分值：10 分"]
_D5A_TITLE = "评标办法附表（全扫描示例件——双栏排版）"


def gen_d5a(out: Path) -> dict:
    path = out / "D5a-全扫描双栏盖章.pdf"
    W, H = int(A4[0]), int(A4[1])
    img = Image.new("RGB", (W, H), (250, 249, 246))
    d = ImageDraw.Draw(img, "RGBA")
    font_t = ImageFont.truetype(NOTO_PATH, 52)
    font_h = ImageFont.truetype(NOTO_PATH, 30)
    font_b = ImageFont.truetype(NOTO_PATH, 26)
    d.text((60, 70), _D5A_TITLE, fill=(25, 25, 25), font=font_t)
    # 双栏
    y = 200
    for left, right in zip(_D5A_LEFT, _D5A_RIGHT):
        d.text((70, y), left, fill=(40, 40, 40), font=font_b)
        d.text((430, y), right, fill=(40, 40, 40), font=font_b)
        y += 64
    # 分栏线
    d.rectangle((400, 190, 402, y - 20), fill=(120, 120, 120))
    # 半透明印章遮挡（红色圆 + 内圈 + 竖排文字示意）
    cx, cy, r = W // 2 + 20, H // 2 + 130, 150
    d.ellipse((cx - r, cy - r, cx + r, cy + r), outline=(200, 30, 30, 110), width=10)
    d.ellipse((cx - r + 18, cy - r + 18, cx + r - 18, cy + r - 18), outline=(200, 30, 30, 90), width=4)
    stamp_font = ImageFont.truetype(NOTO_PATH, 34)
    d.text((cx - 52, cy - 60), "示例专用章", fill=(200, 30, 30, 130), font=stamp_font)
    d.text((cx - 46, cy + 10), "（遮挡区域）", fill=(200, 30, 30, 120), font=font_b)
    # 像素自证（防可变字体静默空白）：双栏文本+印章必须真实落在像素层
    dark = _assert_pixels_text(img, 20000, "D5a")

    c = rl_canvas.Canvas(str(path), pagesize=A4)
    c.setTitle("D5a 全扫描双栏盖章（结构测试件）")
    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    buf.seek(0)
    from reportlab.lib.utils import ImageReader
    c.drawImage(ImageReader(buf), 0, 0, width=A4[0], height=A4[1])
    c.showPage()
    c.save()
    return {"path": path.name, "text_layer": "none", "columns": 2, "stamp_occlusion": True}


_D5B_P1 = [
    "澄清通知（图文混排示例件）",
    "正文第一页含可提取文本层：本通知是对招标文件的澄清与补充。",
    "一、原招标文件第五章 5.2 条中的技术参数由文本层载明，与附表不一致时以本通知为准。",
    "二、附表（见次页图像页）为扫描版格式样例，无文本层。",
    "三、本通知不构成对投标截止时间的变更。",
]


def gen_d5b(out: Path) -> dict:
    path = out / "D5b-图文混排.pdf"
    # 第 1 页：文本层
    c = rl_canvas.Canvas(str(path), pagesize=A4)
    c.setTitle("D5b 图文混排（结构测试件）")
    pdfmetrics_font = "STSong-Light"
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.cidfonts import UnicodeCIDFont
    if pdfmetrics_font not in pdfmetrics.getRegisteredFontNames():
        pdfmetrics.registerFont(UnicodeCIDFont(pdfmetrics_font))
    y = A4[1] - 60 * mm
    for i, line in enumerate(_D5B_P1):
        c.setFont(pdfmetrics_font, 24 if i == 0 else 13)
        c.drawString(50, y, line)
        y -= 18 if i == 0 else 22
    c.showPage()
    # 第 2 页：纯图像（无文本层）
    W, H = int(A4[0]), int(A4[1])
    img = Image.new("RGB", (W, H), (252, 251, 249))
    d = ImageDraw.Draw(img)
    font_t = ImageFont.truetype(NOTO_PATH, 44)
    font_b = ImageFont.truetype(NOTO_PATH, 26)
    d.text((70, 90), "附表：响应偏离表（图像页——无文本层）", fill=(30, 30, 30), font=font_t)
    yy = 210
    for k in range(10):
        d.text((80, yy), f"序号 {k + 1}｜条款（示例）｜响应：完全响应", fill=(45, 45, 45), font=font_b)
        yy += 52
    # 像素自证（防可变字体静默空白）：附表文本必须真实落在像素层
    _assert_pixels_text(img, 8000, "D5b-附表页")
    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    buf.seek(0)
    from reportlab.lib.utils import ImageReader
    c.drawImage(ImageReader(buf), 0, 0, width=A4[0], height=A4[1])
    c.showPage()
    c.save()
    return {"path": path.name, "text_layer": "partial", "pages": 2,
            "page1_text": True, "page2_image_only": True}


# ---------------------------------------------------------------- 汇总入口
def gen_corpus_r(corpus_r_dir: Path) -> dict:
    d1 = corpus_r_dir / "D1-tables"
    d2 = corpus_r_dir / "D2-nonbody"
    d5 = corpus_r_dir / "D5-scanned"
    for d in (d1, d2, d5):
        d.mkdir(parents=True, exist_ok=True)
    layers = {
        "D1-tables": [gen_d1a(d1), gen_d1b(d1)],
        "D2-nonbody": [gen_d2a(d2)],
        "D5-scanned": [gen_d5a(d5), gen_d5b(d5)],
    }
    (corpus_r_dir / "README.md").write_text(README, encoding="utf-8")
    files = [f["path"] for rows in layers.values() for f in rows]
    assert sorted(Path(f).name for f in files) == sorted(Path(f).name for f in OUTPUT_FILES), \
        f"corpus-R 产出与 OUTPUT_FILES 漂移：{files}"
    return {"version": CORPUS_R_VERSION, "files": files, "layers": layers,
            "counts": {"D1": 2, "D2": 1, "D5": 2, "total": len(files)}}


if __name__ == "__main__":
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("fixtures/corpus-R")
    print(gen_corpus_r(out)["counts"])
