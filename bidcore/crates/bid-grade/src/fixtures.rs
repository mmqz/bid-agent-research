//! 夹具数据装载：GT（requirements.jsonl）/ 查询（exact-25.jsonl）/ 预期转换（TC-*.json）。
//! 数据契约单一真源 = fixture_spec.py 生成的 jsonl（语言中立的冻结数据），Rust 只读不改。

use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct GtRequirement {
    pub gt_id: String,
    pub clause_no: Option<Value>, // 字符串或数组（合并口径）
    pub category: String,
    pub anchors: Vec<GtAnchor>,
    pub params: Value,
    pub expected_policy: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GtAnchor {
    pub file: String,
    pub kind: String,
    pub locator: String,
    pub excerpt: String,
}

#[derive(Debug, Clone)]
pub struct EtAssert {
    pub kind: String,
    pub path: String,
    pub op: Option<String>,
    pub value: Value,
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct EtScenario {
    pub tc_id: String,
    pub asserts: Vec<EtAssert>,
}

pub struct Fixtures {
    pub root: PathBuf,
    pub gts: Vec<GtRequirement>,
    pub queries: Vec<Value>,
    pub scenarios: Vec<EtScenario>,
}

impl Fixtures {
    pub fn load(root: &Path) -> Result<Self, String> {
        let gt_path = root.join("corpus-A/ground-truth/requirements.jsonl");
        let mut gts = vec![];
        for line in std::fs::read_to_string(&gt_path).map_err(|e| e.to_string())?.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
            gts.push(GtRequirement {
                gt_id: v["gt_id"].as_str().unwrap_or_default().to_string(),
                clause_no: v.get("clause_no").cloned(),
                category: v["category"].as_str().unwrap_or_default().to_string(),
                anchors: v["anchors"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .map(|a| GtAnchor {
                                file: a["file"].as_str().unwrap_or_default().into(),
                                kind: a["kind"].as_str().unwrap_or_default().into(),
                                locator: a["locator"].as_str().unwrap_or_default().into(),
                                excerpt: a["excerpt"].as_str().unwrap_or_default().into(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                params: v.get("params").cloned().unwrap_or(Value::Null),
                expected_policy: v.get("expected_policy").and_then(|x| x.as_str()).map(String::from),
            });
        }
        let q_path = root.join("queries/exact-25.jsonl");
        let queries: Vec<Value> = std::fs::read_to_string(&q_path)
            .map_err(|e| e.to_string())?
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        let et_dir = root.join("corpus-A/expected-transitions");
        let mut scenarios = vec![];
        for entry in std::fs::read_dir(&et_dir).map_err(|e| e.to_string())?.flatten() {
            let p = entry.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            let asserts = v["asserts"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .map(|a| EtAssert {
                            kind: a["kind"].as_str().unwrap_or_default().into(),
                            path: a["path"].as_str().unwrap_or_default().into(),
                            op: a.get("op").and_then(|x| x.as_str()).map(String::from),
                            value: a.get("value").cloned().unwrap_or(Value::Null),
                            note: a.get("note").and_then(|x| x.as_str()).unwrap_or_default().into(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            scenarios.push(EtScenario {
                tc_id: v["tc_id"].as_str().unwrap_or_default().to_string(),
                asserts,
            });
        }
        scenarios.sort_by(|a, b| a.tc_id.cmp(&b.tc_id));
        Ok(Fixtures { root: root.to_path_buf(), gts, queries, scenarios })
    }

    pub fn scenario(&self, tc_id: &str) -> Option<&EtScenario> {
        self.scenarios.iter().find(|s| s.tc_id == tc_id)
    }
}
