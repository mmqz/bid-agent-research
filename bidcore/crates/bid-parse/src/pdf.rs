//! PDF 解析：文本层（poppler pdftotext）+ 扫描层（pdftoppm → tesseract chi_sim）。
//! 无文本层 → OCR 通道（`has_text_layer=false` + OcrLine 块）；两层皆空 → 诚实留空并说明。

use crate::{Block, BlockKind, ParsedFile};
use std::path::Path;
use std::process::Command;

fn run(cmd: &mut Command) -> Result<String, String> {
    let out = cmd.output().map_err(|e| format!("{:?}: {e}", cmd.get_program()))?;
    if !out.status.success() {
        return Err(format!(
            "{:?} 退出码 {:?}: {}",
            cmd.get_program(),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).chars().take(160).collect::<String>()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn parse(path: &Path) -> Result<ParsedFile, String> {
    let text = run(Command::new("pdftotext")
        .arg("-layout")
        .arg(path.as_os_str())
        .arg("-"))?;
    // pdftotext 以 \f 分页且末页后常带尾随 \f——按「非空分片」计页
    let pages = text
        .split('\u{c}')
        .filter(|seg| !seg.trim().is_empty())
        .count()
        .max(1);
    let non_ws: usize = text.chars().filter(|c| !c.is_whitespace()).count();

    let mut blocks: Vec<Block> = Vec::new();
    let mut has_text_layer = non_ws >= 16; // 空白/仅页码层判为无文本层
    let mut ocr_blocks = 0usize;
    let mut summary = String::new();

    if has_text_layer {
        for (pi, page) in text.split('\u{c}').enumerate() {
            let mut line_no = 0usize;
            for line in page.lines() {
                let l = line.trim();
                if l.is_empty() {
                    continue;
                }
                line_no += 1;
                blocks.push(Block {
                    idx: blocks.len(),
                    kind: BlockKind::PdfLine,
                    locator: format!("第{}页 行{}", pi + 1, line_no),
                    text: collapse_spaces(l),
                    cells: vec![],
                });
            }
        }
        summary = format!("pdf 文本层: {pages} 页 {} 行块", blocks.len());
    } else {
        // OCR 通道：渲染每页 → tesseract chi_sim（分档：native_text 无 / ocr 分档）
        has_text_layer = false;
        let tmp = std::env::temp_dir().join(format!("bidparse_ocr_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let prefix = tmp.join("page");
        let render = Command::new("pdftoppm")
            .args(["-r", "200", "-gray", "-png"])
            .arg(path.as_os_str())
            .arg(&prefix)
            .output()
            .map_err(|e| format!("pdftoppm 不可用: {e}"))?;
        if !render.status.success() {
            return Err(format!(
                "pdftoppm 失败: {}",
                String::from_utf8_lossy(&render.stderr).chars().take(160).collect::<String>()
            ));
        }
        let mut pngs: Vec<std::path::PathBuf> = std::fs::read_dir(&tmp)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("png"))
            .collect();
        pngs.sort();
        for (pi, png) in pngs.iter().enumerate() {
            let tessdata = std::path::PathBuf::from(
                std::env::var("TESSDATA_PREFIX")
                    .unwrap_or_else(|_| format!("{}/.tessdata", std::env::var("HOME").unwrap_or_default())),
            );
            let ocr = Command::new("tesseract")
                .arg(png.as_os_str())
                .arg("stdout")
                .args(["-l", "chi_sim", "--psm", "6"])
                .env("TESSDATA_PREFIX", &tessdata)
                .output()
                .map_err(|e| format!("tesseract 不可用: {e}"))?;
            if !ocr.status.success() {
                summary = format!(
                    "pdf 扫描层: {pages} 页，OCR 失败（{}）",
                    String::from_utf8_lossy(&ocr.stderr).chars().take(80).collect::<String>()
                );
                break;
            }
            let page_text = String::from_utf8_lossy(&ocr.stdout).to_string();
            let mut line_no = 0usize;
            for line in page_text.lines() {
                let l = line.trim();
                if l.chars().filter(|c| !c.is_whitespace()).count() < 2 {
                    continue; // 碎屑噪声行
                }
                line_no += 1;
                ocr_blocks += 1;
                blocks.push(Block {
                    idx: blocks.len(),
                    kind: BlockKind::OcrLine,
                    locator: format!("第{}页 OCR 行{}", pi + 1, line_no),
                    text: collapse_spaces(l),
                    cells: vec![],
                });
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);
        if summary.is_empty() {
            summary = format!("pdf 扫描层 OCR: {pages} 页，{ocr_blocks} 行块（chi_sim）");
        }
    }

    let tables = blocks
        .iter()
        .filter(|b| b.kind == BlockKind::PdfLine && b.text.contains("  "))
        .count(); // -layout 下列对齐的多列表征（粗粒度——PDF 表格结构归 D1 面）
    Ok(ParsedFile {
        name: String::new(),
        sha256: String::new(),
        pages,
        tables,
        has_text_layer,
        ocr_blocks,
        blocks,
        parse_summary: summary,
    })
}

fn collapse_spaces(s: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim_end().to_string()
}
