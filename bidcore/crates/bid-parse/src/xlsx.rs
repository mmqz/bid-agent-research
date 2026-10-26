//! XLSX 解析：calamine 读全工作表行。图纸目录/量价清单的读面（写面属 M3+ 文档内核）。

use crate::{Block, BlockKind, ParsedFile};
use calamine::{open_workbook_auto, Data, Reader};
use std::path::Path;

pub fn parse(path: &Path) -> Result<ParsedFile, String> {
    let mut wb = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let mut blocks: Vec<Block> = Vec::new();
    let sheet_names = wb.sheet_names().to_vec();
    let mut filled_rows = 0usize;

    for sheet in &sheet_names {
        let range = wb.worksheet_range(sheet).map_err(|e| e.to_string())?;
        for (ri, row) in range.rows().enumerate() {
            let cells: Vec<String> = row
                .iter()
                .map(|c| match c {
                    Data::Empty => String::new(),
                    Data::String(s) => s.trim().to_string(),
                    Data::Float(f) => {
                        if f.fract() == 0.0 {
                            format!("{}", *f as i64)
                        } else {
                            format!("{f}")
                        }
                    }
                    Data::Int(i) => format!("{i}"),
                    Data::DateTime(d) => d.to_string(),
                    Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
                    Data::Bool(b) => format!("{b}"),
                    Data::Error(e) => format!("<err {e:?}>"),
                })
                .collect();
            if cells.iter().all(|c| c.is_empty()) {
                continue;
            }
            filled_rows += 1;
            let text = cells
                .iter()
                .filter(|c| !c.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            blocks.push(Block {
                idx: blocks.len(),
                kind: BlockKind::XlsxRow,
                locator: format!("{sheet}!行{}", ri + 1),
                text,
                cells,
            });
        }
    }

    let pages = sheet_names.len().max(1); // 工作表数作「页」（xlsx 无页概念）
    Ok(ParsedFile {
        name: String::new(),
        sha256: String::new(),
        pages,
        tables: sheet_names.len(),
        has_text_layer: true,
        ocr_blocks: 0,
        blocks,
        parse_summary: format!("xlsx: {} 工作表，{filled_rows} 有效行", sheet_names.len()),
    })
}
