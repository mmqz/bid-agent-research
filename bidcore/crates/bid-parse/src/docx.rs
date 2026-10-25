//! DOCX 解析：zip + quick-xml 走 word/document.xml。
//! 输出与文档顺序一致的块序列（段落 / 表格行），并维护章节上下文（章 → 节 → 条款号前缀）
//! 作为 locator —— 下游条款抽取与影响面分析的位置真源。

use crate::guard::{self, LIMIT_ACTUAL_READ_DOCX, LIMIT_INPUT_BYTES_DOCX, LIMIT_XML_DEPTH};
use crate::{Block, BlockKind, ParsedFile};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::path::Path;

pub fn parse(path: &Path) -> Result<ParsedFile, String> {
    // T-03 防御链：输入字节门 → prescan 三门 → 受限读取（实际累计计量）→ XML 深度门
    let mut zf = guard::open_guarded(path, LIMIT_INPUT_BYTES_DOCX)
        .map_err(|e| format!("zip 防护拒绝: {e}"))?;
    let doc_xml = guard::read_entry_limited(&mut zf, "word/document.xml", LIMIT_ACTUAL_READ_DOCX)
        .map_err(|e| format!("读取防护拒绝: {e}"))?;

    let mut reader = Reader::from_str(&doc_xml);
    reader.config_mut().trim_text(true);

    let mut blocks: Vec<Block> = Vec::new();
    let mut chapter = String::new();
    let mut subsection = String::new();

    let mut in_table = false;
    let mut table_idx: usize = 0;
    let mut in_cell = false;
    let mut cell_text = String::new();
    let mut row_cells: Vec<String> = Vec::new();
    let mut row_no: usize = 0;
    let mut para_text = String::new();
    let mut in_para = false;
    let mut capture_text = false;

    // 表格挂在哪个章节上下文（表格开始时冻结当前章节——locator 直接携带）
    let mut tbl_chapter = String::new();

    fn emit_para(blocks: &mut Vec<Block>, idx: usize, chapter: &str, subsection: &str, text: &str) {
        let locator = if chapter.is_empty() {
            String::new()
        } else if subsection.is_empty() {
            chapter.to_string()
        } else {
            format!("{chapter} > {subsection}")
        };
        blocks.push(Block { idx, kind: BlockKind::Para, locator, text: text.to_string(), cells: vec![] });
    }

    // T-03：XML 嵌套深度门（引擎 LIMIT_XML_DEPTH 同构；深嵌套耗内存/CPU 面）
    let mut depth: usize = 0;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                depth += 1;
                if depth > LIMIT_XML_DEPTH {
                    return Err(format!(
                        "XML 嵌套深度超限 {depth} > {LIMIT_XML_DEPTH}（防护拒绝）"));
                }
                match e.name().as_ref() {
                b"w:tbl" => {
                    in_table = true;
                    table_idx += 1;
                    row_no = 0;
                    tbl_chapter = chapter.clone();
                }
                b"w:tr" if in_table => {
                    row_no += 1;
                    row_cells.clear();
                }
                b"w:tc" if in_table => {
                    in_cell = true;
                    cell_text.clear();
                }
                b"w:p" => {
                    in_para = true;
                    para_text.clear();
                }
                b"w:t" | b"w:tab" => {
                    capture_text = true;
                }
                _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                if capture_text {
                    let txt = t.unescape().map_err(|e| e.to_string())?;
                    if in_cell {
                        cell_text.push_str(&txt);
                    } else if in_para {
                        para_text.push_str(&txt);
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                // 空元素（如 <w:tab/>）：对 tab 补一个空格分隔符
                if e.name().as_ref() == b"w:tab" && in_para && !in_cell {
                    para_text.push(' ');
                }
            }
            Ok(Event::End(e)) => {
                // 递减先于 match——`continue` 跳段也不会漏计（saturating 防负）
                depth = depth.saturating_sub(1);
                match e.name().as_ref() {
                b"w:p" => {
                    in_para = false;
                    capture_text = false;
                    if in_cell {
                        cell_text.push(' '); // 单元格内多段以空格连接
                    } else {
                        let t = para_text.trim().to_string();
                        if t.is_empty() {
                            continue;
                        }
                        // 章节上下文更新（按文本模式——fixture 与政采范本惯例）
                        if let Some(c) = as_chapter(&t) {
                            chapter = c;
                            subsection.clear();
                        } else if let Some(s) = as_subsection(&t) {
                            subsection = s;
                        }
                        let idx = blocks.len();
                        emit_para(&mut blocks, idx, &chapter, &subsection, &t);
                    }
                }
                b"w:tc" => {
                    in_cell = false;
                    capture_text = false;
                    row_cells.push(cell_text.trim().to_string());
                }
                b"w:tr" => {
                    if !row_cells.iter().all(|c| c.trim().is_empty()) {
                        let locator = format!("{tbl_chapter} 表{table_idx} 行{row_no}");
                        let text = row_cells
                            .iter()
                            .filter(|c| !c.trim().is_empty())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" ");
                        blocks.push(Block {
                            idx: blocks.len(),
                            kind: BlockKind::TableRow,
                            locator,
                            text,
                            cells: row_cells.clone(),
                        });
                    }
                }
                b"w:tbl" => {
                    in_table = false;
                }
                _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(format!("XML 事件错误: {e}")),
        }
    }

    // 页数估算：DOCX 无真实分页（渲染口径页数属 M4 导出面）；以结构页（章）计
    let pages = 1 + blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Para && as_chapter(&b.text).is_some())
        .count();
    let tables = table_idx;
    Ok(ParsedFile {
        name: String::new(),
        sha256: String::new(),
        pages,
        tables,
        has_text_layer: true,
        ocr_blocks: 0,
        blocks,
        parse_summary: format!("docx: {tables} 表格，段落+表格行块 {cnt}", cnt = 0),
    })
    .map(|mut p| {
        let cnt = p.blocks.len();
        p.parse_summary = format!("docx: {tables} 表格，{cnt} 块");
        p
    })
}

fn cn_num_ok(s: &str) -> bool {
    s.chars().all(|c| "一二三四五六七八九十百".contains(c))
}

fn as_chapter(t: &str) -> Option<String> {
    // 「第X章 标题」——章号用中文数字；携带完整标题（类别映射与 GT locator 口径需要全文）
    if t.len() >= 4 && t.starts_with("第") {
        if let Some(pos) = t.find("章") {
            let num = &t[3..pos]; // "第" 占 3 字节
            if !num.is_empty() && cn_num_ok(num) && pos + 3 <= t.len() {
                return Some(t.trim().to_string());
            }
        }
    }
    None
}

fn as_subsection(t: &str) -> Option<String> {
    // 「一、标题」——中文序号节标题（长度短，避免把正文当标题）；携带完整标题
    if t.chars().count() <= 30 {
        if let Some(idx) = t.find('、') {
            let head = &t[..idx];
            if !head.is_empty() && cn_num_ok(head) {
                return Some(t.trim().to_string());
            }
        }
    }
    None
}

pub fn is_numbered_clause(t: &str) -> bool {
    // 「3.2.1 正文…」——层级条款号开头（数字开头且含点）
    let head: String = t.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    head.contains('.') && head.len() >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_subsection_patterns() {
        assert!(as_chapter("第三章 投标人须知").is_some());
        assert!(as_chapter("第十二章 合同").is_some());
        assert!(as_chapter("3.1.1 正文不是章").is_none());
        assert!(as_subsection("七、开标、评标与否决条款").is_some());
        assert!(as_subsection("二、投标人资格要求").is_some());
        assert!(as_subsection("评标办法（综合评分法）").is_none());
        assert!(is_numbered_clause("3.2.1 投标人须具有独立承担民事责任能力的法人营业执照"));
        assert!(is_numbered_clause("4.1 价格分（30 分）"));
        assert!(!is_numbered_clause("投标文件格式"));
    }
}
