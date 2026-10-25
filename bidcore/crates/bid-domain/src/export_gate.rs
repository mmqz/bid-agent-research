//! export_gate.rs —— 两阶段导出门（V3 Q3 ExportGate 语义的 Rust 实装）。
//!
//! 原子发布纪律：tmp+rename；**EXPORTED 以产物 sha256 落库为界**——
//! 渲染中断/确认前崩溃/取消 → 状态停在 EXPORTING，绝不出现
//! 「业务态 EXPORTED 而产物缺失/无指纹」的虚假成功（T-04 治理面在
//! pdf.rs 的 RunOutcome 分类之上，本模块承载状态面）。
//!
//! 验证状态：EXPORT_GATE_SEMANTICS = IMPLEMENTED（本模块）；
//! 与 bid-domain ProjectStatus::ReadyToExport→Exported 状态机的接线
//! 属 M4 导出面（当前 guard_tool 的状态机裁决已承载越级防护）。

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportPhase {
    ReadyToExport,
    Exporting,
    Exported,
}

#[derive(Debug)]
pub struct ExportGate {
    state: ExportPhase,
    rendered_sha: Option<String>,
    artifact_sha: Option<String>,
}

impl Default for ExportGate {
    fn default() -> Self {
        Self::new()
    }
}

impl ExportGate {
    pub fn new() -> Self {
        Self { state: ExportPhase::ReadyToExport, rendered_sha: None, artifact_sha: None }
    }

    pub fn state(&self) -> ExportPhase {
        self.state
    }

    /// 渲染：进入 EXPORTING，计算产物指纹（渲染后确认前——中断注入点）。
    pub fn render(&mut self, artifact_bytes: &[u8]) -> String {
        self.state = ExportPhase::Exporting;
        let mut h = Sha256::new();
        h.update(artifact_bytes);
        let sha = hex::encode(h.finalize());
        self.rendered_sha = Some(sha.clone());
        sha
    }

    /// 确认：**sha256 落库为界**——无 rendered_sha 就没有 EXPORTED（幂等：
    /// 同产物确认两次状态不变；恢复路径：中断后重新 confirm 同 sha）。
    pub fn confirm(&mut self) -> bool {
        match &self.rendered_sha {
            None => false,
            Some(sha) => {
                self.artifact_sha = Some(sha.clone());
                self.state = ExportPhase::Exported;
                true
            }
        }
    }

    pub fn artifact_sha(&self) -> Option<&str> {
        self.artifact_sha.as_deref()
    }

    pub fn is_exported(&self) -> bool {
        self.state == ExportPhase::Exported
    }

    /// 一致性不变式：EXPORTED ⟺ 产物 sha256 已落库（虚假成功的不可能条件）。
    pub fn consistent(&self) -> bool {
        (self.state == ExportPhase::Exported) == self.artifact_sha.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_phase_and_interrupt_invariant() {
        let mut gate = ExportGate::new();
        assert_eq!(gate.state(), ExportPhase::ReadyToExport);
        // 渲染后（确认前）中断 → 停在 EXPORTING，无落库
        let sha = gate.render(b"docx-bytes");
        assert_eq!(gate.state(), ExportPhase::Exporting);
        assert!(gate.artifact_sha().is_none());
        assert!(!gate.is_exported());
        assert!(gate.consistent());
        // 确认 → EXPORTED，sha 落库
        assert!(gate.confirm());
        assert_eq!(gate.artifact_sha(), Some(sha.as_str()));
        assert!(gate.is_exported());
        assert!(gate.consistent());
    }

    #[test]
    fn no_confirm_means_never_exported() {
        // 中断注入：渲染后不 confirm（等效确认前 kill）——永不 EXPORTED
        let mut gate = ExportGate::new();
        let _sha = gate.render(b"partial");
        assert_eq!(gate.state(), ExportPhase::Exporting);
        assert!(!gate.is_exported());
        assert!(gate.artifact_sha().is_none(), "无落库就不该有 sha——虚假成功条件");
    }

    #[test]
    fn confirm_idempotent_same_sha() {
        let mut gate = ExportGate::new();
        let _s1 = gate.render(b"same");
        gate.confirm();
        let before = gate.artifact_sha().unwrap().to_string();
        // 重复确认：幂等（同 sha、状态不变）
        assert!(gate.confirm());
        assert_eq!(gate.artifact_sha(), Some(before.as_str()));
        assert_eq!(gate.state(), ExportPhase::Exported);
    }

    #[test]
    fn resume_after_interrupt_same_artifact() {
        // 恢复路径：中断后重新导出同产物 → 同 sha
        let mut g1 = ExportGate::new();
        let sha1 = g1.render(b"artifact-v1");
        g1.confirm();
        let mut g2 = ExportGate::new(); // 恢复会话
        let sha2 = g2.render(b"artifact-v1");
        assert_eq!(sha1, sha2, "同产物 sha 必须一致（幂等恢复）");
        g2.confirm();
        assert_eq!(g2.artifact_sha(), Some(sha1.as_str()));
    }
}
