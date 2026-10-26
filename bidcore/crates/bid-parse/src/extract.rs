//! 确定性条款抽取（extract_requirements 的「确定性先行」层——mcp-contract §3）。
//!
//! M1 纪律：零模型调用。类别判定 = 章节上下文映射（章/节标题 → 类别），
//! 参数抽取 = 正则槽位（型号/金额/日期），条款号 = 编号头解析 + 表格行位。
//! 模型补全面（语义归类、陈述规范化）属 M2+——本层输出即 REQ 的稳定基座。
//!
//! 类别映射（章节上下文——政采范本结构惯例，规则钉死可审计）：
//!   评标办法→评分项；技术需求→技术参数；商务要求→商务要求；
//!   须知+资格→资格条件；须知+否决→否决项；须知+编制→格式要求；其余→其他。

use crate::{Block, BlockKind, ParsedFile};
use bid_domain::{Anchor, Requirement};
use regex::Regex;
use std::collections::HashMap;

/// 型号谱系正则：字母开头（≥1 位）+ 连字符分段（防误配纯数字编号如「附件 7-3」）。
/// 覆盖实测谱系：GW-8000-T4 / S5735-L48T4X-A1 / USG6650E-AC550 / IPC-4T-IR-D2 / NVR-64H-8P / DCS-4000。
fn model_regex() -> Regex {
    Regex::new(r"[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+").unwrap()
}

pub fn extract_requirements(files: &[ParsedFile]) -> Vec<Requirement> {
    let mut reqs: Vec<Requirement> = Vec::new();
    let mut seq = 0usize;
    for f in files {
        match () {
            _ if f.name.starts_with("A-") => extract_main_docx(f, &mut reqs, &mut seq),
            _ if f.name.starts_with("C-") => extract_main_docx(f, &mut reqs, &mut seq), // 补遗同构（增量面属 V2）
            _ if f.name.starts_with("B1") => extract_b1_lines(f, &mut reqs, &mut seq),
            _ if f.name.starts_with("B2") => extract_b2_ocr(f, &mut reqs, &mut seq),
            _ => {} // B3 图纸目录/证据件：无条款抽取面（证据入库走 chunk 索引）
        }
    }
    reqs
}

fn category_for(chapter: &str, subsection: &str) -> &'static str {
    if chapter.contains("评标办法") {
        "评分项"
    } else if chapter.contains("技术需求") || chapter.contains("技术规范") {
        "技术参数"
    } else if chapter.contains("商务要求") {
        "商务要求"
    } else if chapter.contains("须知") && subsection.contains("资格") {
        "资格条件"
    } else if chapter.contains("须知") && subsection.contains("否决") {
        "否决项"
    } else if chapter.contains("须知") && subsection.contains("编制") {
        "格式要求"
    } else {
        "其他"
    }
}

/// 主文件（A）与补遗（C）：编号条款段落 + 前附表否决行 + 技术需求参数表。
fn extract_main_docx(f: &ParsedFile, reqs: &mut Vec<Requirement>, seq: &mut usize) {
    let mut last_numbered_head: Option<String> = None; // 表格行收养最近的编号段（5.x 参数表上下文）
    let re_clause = Regex::new(r"^(\d+(?:\.\d+)+)\s*").unwrap();
    let re_model = model_regex();
    let re_amount = Regex::new(r"(\d+(?:\.\d+)?)\s*万\s*元").unwrap();
    let re_date = Regex::new(r"\d{4}-\d{2}-\d{2}").unwrap();
    let re_date_cn = Regex::new(r"[〇一二三四五六七八九零]{4}年[〇一二三四五六七八九零]{1,2}月[〇一二三四五六七八九零]{1,3}日").unwrap();

    for b in &f.blocks {
        match b.kind {
            BlockKind::Para => {
                if let Some(m) = re_clause.captures(&b.text) {
                    last_numbered_head = Some(m[1].to_string());
                    let (chapter, subsection) = split_locator(&b.locator);
                    let mut params = HashMap::new();
                    fill_params(&b.text, &re_model, &re_amount, &re_date, &re_date_cn, &mut params);
                    push_req(reqs, seq, category_for(chapter, subsection), Some(m[1].to_string()),
                             vec![anchor(f, b)], params, &f.name);
                }
            }
            BlockKind::TableRow => {
                let joined = b.cells.join("");
                if b.locator.contains("前附表") && (joined.contains("否决") || b.text.contains("否决")) {
                    // 前附表否决行：条款号 = 前附表-{序号}（序号列为准；缺序号回退 XML 行号）
                    let item = b.cells.iter()
                        .find(|c| c.chars().all(|x| x.is_ascii_digit()) && !c.is_empty())
                        .and_then(|c| c.parse::<usize>().ok())
                        .unwrap_or_else(|| row_no_of(&b.locator));
                    push_req(reqs, seq, "否决项", Some(format!("前附表-{item}")),
                             vec![anchor(f, b)], HashMap::new(), &f.name);
                } else if (b.locator.contains("技术需求") || b.locator.contains("第五章"))
                    && re_model.is_match(&b.text)
                {
                    // 参数表行：收养最近的 5.x 编号头作条款号
                    let mut params = HashMap::new();
                    fill_params(&b.text, &re_model, &re_amount, &re_date, &re_date_cn, &mut params);
                    push_req(reqs, seq, "技术参数", last_numbered_head.clone(),
                             vec![anchor(f, b)], params, &f.name);
                }
            }
            _ => {}
        }
    }
}

/// B1 设备清单：含型号或中文大写日期的行 → 技术参数 REQ（到货/规格要求行）。
fn extract_b1_lines(f: &ParsedFile, reqs: &mut Vec<Requirement>, seq: &mut usize) {
    let re_model = model_regex();
    let re_date_cn = Regex::new(r"[〇一二三四五六七八九零]{4}年[〇一二三四五六七八九零]{1,2}月[〇一二三四五六七八九零]{1,3}日").unwrap();
    let re_amount = Regex::new(r"(\d+(?:\.\d+)?)\s*万\s*元").unwrap();
    let re_date = Regex::new(r"\d{4}-\d{2}-\d{2}").unwrap();
    for b in &f.blocks {
        if matches!(b.kind, BlockKind::PdfLine)
            && (re_model.is_match(&b.text) || re_date_cn.is_match(&b.text))
        {
            let mut params = HashMap::new();
            fill_params(&b.text, &re_model, &re_amount, &re_date, &re_date_cn, &mut params);
            push_req(reqs, seq, "技术参数", None, vec![anchor(f, b)], params, &f.name);
        }
    }
}

/// B2 扫描件：否决关键词 OCR 行 + 1 行续接窗 → 否决项 REQ（扫描件内条款不丢的抽取面）。
fn extract_b2_ocr(f: &ParsedFile, reqs: &mut Vec<Requirement>, seq: &mut usize) {
    let keywords = ["否决", "签署", "盖章"];
    let blocks = &f.blocks;
    for (i, b) in blocks.iter().enumerate() {
        if !matches!(b.kind, BlockKind::OcrLine) {
            continue;
        }
        if !keywords.iter().any(|k| b.text.contains(k)) {
            continue;
        }
        // 种子行 + 下一行拼接（断行条款还原）；excerpt 同时携带两行原文
        let mut excerpt = b.text.clone();
        let mut locator = b.locator.clone();
        if let Some(next) = blocks.get(i + 1) {
            if matches!(next.kind, BlockKind::OcrLine) && next.text.chars().count() >= 2 {
                excerpt.push_str(&next.text);
                locator.push_str(" + 行接");
            }
        }
        push_req(reqs, seq, "否决项", None,
                 vec![Anchor { file: f.name.clone(), kind: "ocr".into(), locator, excerpt }],
                 HashMap::new(), &f.name);
    }
}

fn push_req(reqs: &mut Vec<Requirement>, seq: &mut usize, category: &str,
            clause_no: Option<String>, anchors: Vec<Anchor>,
            params: HashMap<String, serde_json::Value>, source: &str) {
    *seq += 1;
    reqs.push(Requirement {
        id: format!("REQ-{seq:04}"),
        category: category.to_string(),
        clause_no,
        anchors,
        params,
        source_file: source.to_string(),
    });
}

fn anchor(f: &ParsedFile, b: &Block) -> Anchor {
    Anchor {
        file: f.name.clone(),
        kind: match b.kind {
            BlockKind::Para => "para",
            BlockKind::TableRow => "table",
            BlockKind::PdfLine => "pdf_line",
            BlockKind::OcrLine => "ocr",
            BlockKind::XlsxRow => "xlsx_row",
        }
        .to_string(),
        locator: b.locator.clone(),
        excerpt: b.text.clone(), // 溯源锚原文（创建时固化）
    }
}

fn fill_params(text: &str, re_model: &Regex, re_amount: &Regex, re_date: &Regex,
               re_date_cn: &Regex, params: &mut HashMap<String, serde_json::Value>) {
    if let Some(m) = re_model.find(text) {
        params.insert("model".into(), serde_json::Value::String(m.as_str().to_string()));
    }
    if let Some(m) = re_amount.captures(text) {
        let v: f64 = m[1].parse().unwrap_or(0.0);
        params.insert("amount_wan".into(), serde_json::json!(v));
    }
    if let Some(m) = re_date.find(text) {
        params.insert("date".into(), serde_json::Value::String(m.as_str().to_string()));
    }
    if let Some(m) = re_date_cn.find(text) {
        params.insert("date_cn".into(), serde_json::Value::String(m.as_str().to_string()));
    }
}

fn split_locator(locator: &str) -> (&str, &str) {
    match locator.split_once(" > ") {
        Some((c, s)) => (c, s),
        None => (locator, ""),
    }
}

fn row_no_of(locator: &str) -> usize {
    locator.rsplit("行").next().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}
