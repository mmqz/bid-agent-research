//! XLSX 解析：calamine 读全工作表行。图纸目录/量价清单的读面（写面属 M3+ 文档内核）。
//!
//! T-03 防护边界（诚实登记）：calamine 是黑盒全量内存加载，实际解压流不可注入计量；
//! 本层在入口做三道防线——输入字节门 + zip prescan 三门 + 压缩比门。
//! 残余面（声明值撒谎骗过 prescan 后 calamine 内部解压）登记 threat-model T-03。

use crate::guard::{self, LIMIT_INPUT_BYTES_XLSX};
use crate::{Block, BlockKind, ParsedFile};
use calamine::{open_workbook_auto, Data, Reader};
use std::path::Path;

pub fn parse(path: &Path) -> Result<ParsedFile, String> {
    // T-03：输入字节门 + prescan（条目数/单声明/总声明/压缩比/异常名）。
    // 打开后立即丢弃归档句柄——calamine 自行重开；此处只做拒绝裁决。
    guard::open_guarded(path, LIMIT_INPUT_BYTES_XLSX)
        .map_err(|e| format!("zip 防护拒绝: {e}"))?;
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
