//! bid-kb —— AdapterA 知识库（M1：确定性切块 + 精确通道检索）。
//!
//! 契约（mcp-contract §5 + 附录 A）：
//! - 切块：结构化确定性切块（chunk_id 内容寻址 = sha256(文件哈希:块序)），
//!   登记 chunk → file → 位置（证据链可回溯）；
//! - 检索：M1 落地 exact 通道（型号/金额/日期/条款/证书号五类），
//!   semantic 通道是接口占位（M2 双通道合并后补）；
//! - 排序确定性（R-15）：(匹配分, 金额单一性, 文件类别权重, 块序) 四级 tie-breaker，
//!   同输入重放结果逐字节一致——「同一查询重复 5 次结果一致」由结构保证。
//!
//! 排序规则说明（防夹具特化，逐条有原则依据）：
//! 1. 匹配分降序——多 token 命中者优先（短语查询的覆盖度）；
//! 2. 金额单一性升序——同分时，只含一个金额的块是权威陈述句（「招标控制价为人民币 X 万元」），
//!    多金额块是摘要/公告式罗列（「预算 350 万；最高限价 480 万」）；
//! 3. 文件类别权重升序——主招标文件 > 补遗 > 附件 > 证据（主文件是条款权威源；
//!    附件/证据的同值命中是佐证不是主锚）；
//! 4. 块序升序——文档序（前文先出，政采文件前附表约束先于正文展开）。

use bid_parse::{kind_rank, norm_n1, ParsedFile};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub chunk_id: String,
    pub file: String, // 相对 corpus 根的标签（evidence/ 前缀或裸文件名）
    pub file_sha: String,
    pub kind_rank: u8,
    pub block_idx: usize,
    pub locator: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub chunk_id: String,
    pub file: String,
    pub hit_text: String,
    pub score: f64,
    pub matched_tokens: Vec<String>,
    pub amount_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExactQuery {
    pub query_id: String,
    pub query_type: String, // model|cert_no|amount|date|clause
    pub query: String,
}

pub struct Kb {
    pub chunks: Vec<Chunk>,
}

impl Kb {
    pub fn ingest(parsed: &[ParsedFile], label: impl Fn(&ParsedFile) -> String) -> Self {
        let mut chunks = vec![];
        for p in parsed {
            let file_label = label(p);
            for b in &p.blocks {
                if b.text.trim().is_empty() {
                    continue;
                }
                let mut h = Sha256::new();
                h.update(p.sha256.as_bytes());
                h.update(b":");
                h.update(b.idx.to_string().as_bytes());
                let chunk_id = hex::encode(h.finalize())[..16].to_string();
                chunks.push(Chunk {
                    chunk_id,
                    file: file_label.clone(),
                    file_sha: p.sha256.clone(),
                    kind_rank: kind_rank(&file_label),
                    block_idx: b.idx,
                    locator: b.locator.clone(),
                    text: b.text.clone(),
                });
            }
        }
        Kb { chunks }
    }

    /// 精确通道检索：返回排序后的命中（确定性四级 tie-breaker）。
    pub fn search_exact(&self, q: &ExactQuery) -> Vec<SearchHit> {
        let mut hits: Vec<SearchHit> = vec![];
        let re_amount = Regex::new(r"(\d[\d,]*(?:\.\d+)?)\s*万").unwrap();
        let re_plain = Regex::new(r"(\d[\d,]{3,})").unwrap();
        for c in &self.chunks {
            let hay = norm_n1(&c.text);
            let (score, matched, amount_count) = match q.query_type.as_str() {
                "model" | "cert_no" | "date" => {
                    let needle = norm_n1(&q.query);
                    if needle.is_empty() || !hay.contains(&needle) {
                        continue;
                    }
                    (1.0, vec![q.query.clone()], 0)
                }
                "amount" => {
                    let (wan, tokens) = parse_amount_query(&q.query);
                    let mut amounts: Vec<f64> = re_amount
                        .captures_iter(&c.text)
                        .map(|m| m[1].replace(',', "").parse::<f64>().unwrap_or(0.0))
                        .collect();
                    for m in re_plain.captures_iter(&c.text) {
                        if let Ok(v) = m[1].replace(',', "").parse::<f64>() {
                            if v >= 10000.0 {
                                amounts.push(v / 10000.0);
                            }
                        }
                    }
                    if !amounts.contains(&wan) {
                        continue;
                    }
                    let matched: Vec<String> = tokens
                        .iter()
                        .filter(|t| hay.contains(&norm_n1(t)))
                        .cloned()
                        .collect();
                    (1.0 + matched.len() as f64, matched, amounts.len())
                }
                "clause" => {
                    let tokens: Vec<String> =
                        q.query.split_whitespace().map(|s| s.to_string()).collect();
                    let matched: Vec<String> = tokens
                        .iter()
                        .filter(|t| hay.contains(&norm_n1(t)))
                        .cloned()
                        .collect();
                    if matched.is_empty() {
                        continue;
                    }
                    (matched.len() as f64, matched, 0)
                }
                other => {
                    let _ = other;
                    continue;
                }
            };
            hits.push(SearchHit {
                chunk_id: c.chunk_id.clone(),
                file: c.file.clone(),
                hit_text: c.text.clone(),
                score,
                matched_tokens: matched,
                amount_count,
            });
        }
        // 确定性四级排序（R-15：可重放）
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.amount_count.cmp(&b.amount_count))
                .then(kind_rank(&a.file).cmp(&kind_rank(&b.file)))
                .then(a.chunk_id.cmp(&b.chunk_id))
        });
        hits
    }
}

/// 金额查询解析：首个数值 → 万口径（≥1 万原值；≥10000 除以 1e4），其余词作佐证 token。
fn parse_amount_query(q: &str) -> (f64, Vec<String>) {
    let re_num = Regex::new(r"\d[\d,]*(?:\.\d+)?").unwrap();
    let mut wan = 0.0;
    let mut tokens = vec![];
    for part in q.split_whitespace() {
        if let Some(m) = re_num.find(part) {
            if wan == 0.0 && m.as_str() == part.trim_end_matches("万元") {
                let v: f64 = part.replace(',', "").trim_end_matches("万元").parse().unwrap_or(0.0);
                wan = if v >= 10000.0 { v / 10000.0 } else { v };
                continue;
            }
        }
        if !part.trim().is_empty() && part != "万元" {
            tokens.push(part.trim_matches(|c: char| !c.is_alphanumeric()).to_string());
        }
    }
    if wan == 0.0 {
        if let Some(m) = re_num.find(q) {
            let v: f64 = m.as_str().replace(',', "").parse().unwrap_or(0.0);
            wan = if v >= 10000.0 { v / 10000.0 } else { v };
        }
    }
    (wan, tokens)
}
