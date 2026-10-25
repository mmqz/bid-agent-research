//! PDF 解析：文本层（poppler pdftotext）+ 扫描层（pdftoppm → tesseract chi_sim）。
//! 无文本层 → OCR 通道（`has_text_layer=false` + OcrLine 块）；两层皆空 → 诚实留空并说明。
//!
//! T-04 子进程治理（guard::run_limited）：三外部命令全部受治——超时杀进程组、
//! stdout 上限、结果五分类（超时/溢出/非零/Busy/Spawn 均**不误标成功**）；
//! 临时 PNG 区 TmpGuard RAII 统一回收；扫描页数超 [`MAX_OCR_PAGES`] 整体显式
//! 拒绝（不留可被误认为完整 OCR 的部分结果）。

use crate::guard::{
    self, LIMIT_PDFTOTEXT_STDOUT, LIMIT_TESSERACT_STDOUT, MAX_OCR_PAGES, TIMEOUT_PDFTOPPM,
    TIMEOUT_PDFTOTEXT, TIMEOUT_TESSERACT, RunOutcome, TmpGuard,
};
use crate::{Block, BlockKind, ParsedFile};
use std::path::Path;
use std::process::Command;

pub fn parse(path: &Path) -> Result<ParsedFile, String> {
    let mut text_cmd = Command::new("pdftotext");
    text_cmd.arg("-layout").arg(path.as_os_str()).arg("-");
    let text = guard::run_limited(text_cmd, TIMEOUT_PDFTOTEXT, LIMIT_PDFTOTEXT_STDOUT)
        .into_result()?;
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
        // T-04：扫描页数门——超限整体显式拒绝（渲染/OCR 成本与磁盘占用上限；
        // 拒绝整个文件，绝不产出「只 OCR 了前 N 页」的部分结果）
        if pages > MAX_OCR_PAGES {
            return Err(format!(
                "pdf 扫描层 {pages} 页超过 OCR 上限 {MAX_OCR_PAGES}（拒绝处理——不留部分结果，请拆分文件）"
            ));
        }
        has_text_layer = false;
        // T-04：TmpGuard RAII——超时被杀/任意 Err 提前返回路径均统一回收渲染 PNG
        let tmp = TmpGuard::new("ocr").map_err(|e| format!("临时目录创建失败: {e}"))?;
        let prefix = tmp.0.join("page");
        let mut render_cmd = Command::new("pdftoppm");
        render_cmd
            .args(["-r", "200", "-gray", "-png"])
            .arg(path.as_os_str())
            .arg(&prefix);
        let render = guard::run_limited(render_cmd, TIMEOUT_PDFTOPPM, 4096);
        if !render.is_success() {
            return Err(format!("pdftoppm 失败: {}", short_err(&render)));
        }
        let mut pngs: Vec<std::path::PathBuf> = std::fs::read_dir(&tmp.0)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("png"))
            .collect();
        pngs.sort();
        for (pi, png) in pngs.iter().enumerate() {
            let tessdata = std::path::PathBuf::from(
                std::env::var("TESSDATA_PREFIX").unwrap_or_else(|_| {
                    format!("{}/.tessdata", std::env::var("HOME").unwrap_or_default())
                }),
            );
            let mut t_cmd = Command::new("tesseract");
            t_cmd.arg(png.as_os_str())
                .arg("stdout")
                .args(["-l", "chi_sim", "--psm", "6"])
                .env("TESSDATA_PREFIX", &tessdata);
            let ocr = guard::run_limited(t_cmd, TIMEOUT_TESSERACT, LIMIT_TESSERACT_STDOUT);
            if !ocr.is_success() {
                // 单页 OCR 失败：诚实降级说明（不静默、不中断整档——扫描页残缺属
                // 数据面，parse_summary 承载原因）；超时/溢出类治理失败向上传播拒绝
                match ocr {
                    RunOutcome::Timeout { .. } | RunOutcome::OutputOverflow { .. }
                    | RunOutcome::Busy { .. } | RunOutcome::Spawn { .. } => {
                        return Err(format!("tesseract 治理失败: {}", short_err(&ocr)));
                    }
                    RunOutcome::NonZeroExit { stderr_head, .. } => {
                        summary = format!(
                            "pdf 扫描层: {pages} 页，OCR 失败（{stderr_head}）"
                        );
                        break;
                    }
                    RunOutcome::Success { .. } => unreachable!(),
                }
            }
            let page_text = match ocr {
                RunOutcome::Success { stdout } => stdout,
                _ => unreachable!(),
            };
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

fn short_err(out: &RunOutcome) -> String {
    match out {
        RunOutcome::Timeout { program, secs } => format!("子进程超时被杀（{program} {secs}s）"),
        RunOutcome::OutputOverflow { program, limit } => {
            format!("输出超限（{program} > {limit} 字节）")
        }
        RunOutcome::NonZeroExit { program, code, stderr_head } => {
            format!("{program} 退出码 {code:?}: {stderr_head}")
        }
        RunOutcome::Busy { program } => format!("并发满载（{program}）"),
        RunOutcome::Spawn { program, why } => format!("{program} 无法启动: {why}"),
        RunOutcome::Success { .. } => "成功".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 治理参数语义测试（不依赖外部 PDF 工具——CI 环境兼容）：
    /// 超时/溢出上限值是契约的一部分（变更需过组件版本矩阵审查）。
    #[test]
    fn governance_params_are_pinned() {
        use std::time::Duration;
        assert_eq!(TIMEOUT_PDFTOTEXT, Duration::from_secs(60));
        assert_eq!(TIMEOUT_PDFTOPPM, Duration::from_secs(180));
        assert_eq!(TIMEOUT_TESSERACT, Duration::from_secs(30));
        assert_eq!(LIMIT_PDFTOTEXT_STDOUT, 64 * 1024 * 1024);
        assert_eq!(LIMIT_TESSERACT_STDOUT, 1024 * 1024);
        assert_eq!(MAX_OCR_PAGES, 200);
    }
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
