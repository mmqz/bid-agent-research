//! guard.rs —— 输入资源防御层（S1 威胁模型 T-03/T-04 的实装挂点）。
//!
//! 防护语义两层来源：
//! - 引擎 office-io `io.rs` 的 zip 预扫描门（条目数/单条目声明/总声明/输入字节——
//!   数值对齐引擎 LIMIT_* 家族，docx/xlsx 分族同构）；
//! - S1 威胁模型 T-03：**声明值可撒谎**——中央目录的 size 字段是攻击者可写的，
//!   因此除 prescan 外，实际读取以 [`read_entry_limited`] 流式累计计量，
//!   读出字节超限即失败（真防线，不依赖声明）。
//!
//! 命名检查（异常路径/重复条目/嵌套归档）在 prescan 一并裁决：
//! `../` 穿越、绝对路径、控制字符、超长名、目录深度、同名条目、
//! 内嵌 .zip/.rar/.7z/.tar.* —— 全部显式拒绝（zip slip / 洪水 / 炸弹载具）。
//!
//! 超限行为契约（T-03）：**显式失败、不留部分结果**——调用方在 guard 拒绝时
//! 不得产出任何可被误认为完整解析的产物（Rust `Err` 天然承载，
//! runner 侧落 `ParseStatus::Failed` + E_UNPROCESSABLE 禁引纪律，R-01）。

use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

// ---------------------------------------------------------------- 限额常量
/// docx 族输入字节上限（对齐引擎 LIMIT_INPUT_BYTES_DOCX = 50MB + 2MB 余量）。
pub const LIMIT_INPUT_BYTES_DOCX: u64 = 52 * 1024 * 1024;
/// docx/xlsx 族输入字节上限（对齐引擎 LIMIT_INPUT_BYTES = 300MB + 2MB 余量）。
pub const LIMIT_INPUT_BYTES_XLSX: u64 = 302 * 1024 * 1024;
/// PDF 族输入字节上限（R3-S3-2：docx/xlsx 有门而 PDF 曾无门——任意大小直进 poppler；
/// 500MB 纳入真实大标书 PDF（数百 MB 量级），超限拒绝在读取前）。
pub const LIMIT_INPUT_BYTES_PDF: u64 = 500 * 1024 * 1024;

/// zip 条目数上限（对齐引擎 LIMIT_ENTRY_COUNT）。
pub const LIMIT_ENTRY_COUNT: usize = 10_000;
/// 全部条目未压缩字节总声明上限（对齐引擎 LIMIT_TOTAL_DECLARED）。
pub const LIMIT_TOTAL_DECLARED: u64 = 300 * 1024 * 1024;
/// 单条目未压缩字节声明上限（对齐引擎 LIMIT_SINGLE_ENTRY）。
pub const LIMIT_SINGLE_DECLARED: u64 = 300 * 1024 * 1024;
/// 条目名长度上限（zip 头原生上限 65535，收窄到 UTF-8 文件名常态）。
pub const LIMIT_ENTRY_NAME_LEN: usize = 512;
/// 条目名路径深度上限（`/` 分段数；zip slip 纵深防护）。
pub const LIMIT_PATH_DEPTH: usize = 64;
/// 压缩比上限：未压缩声明 / 已压缩 > 此值判炸弹载具（嵌套炸弹典型 1000:1+）。
pub const LIMIT_COMPRESS_RATIO: u64 = 100;

/// `word/document.xml` 实际读取字节上限（招标文件正文 XML 真实常态 < 30MB；
/// 这是**流式累计**上限，与声明无关——撒谎声明在此被截停）。
pub const LIMIT_ACTUAL_READ_DOCX: u64 = 64 * 1024 * 1024;
/// XML 元素嵌套深度上限（对齐引擎 LIMIT_XML_DEPTH；quadratic 实体不适用
/// quick-xml 流式，但深嵌套内存/CPU 面仍需封顶）。
pub const LIMIT_XML_DEPTH: usize = 1_000;

/// 嵌套归档载具扩展名（外层条目若为归档 → 拒绝：嵌套炸弹的标配形态）。
const NESTED_ARCHIVE_EXTS: &[&str] =
    &["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "tgz", "cab", "lzh", "jar"];

// ---------------------------------------------------------------- 错误面
/// guard 拒绝原因（显式失败——上层转 E_UNPROCESSABLE 语义，绝不静默降级）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardError {
    InputTooLarge { len: u64, limit: u64 },
    EntryCountOver { count: usize, limit: usize },
    EntryNameBad { name: String, reason: &'static str },
    DuplicateEntry { name: String },
    NestedArchive { name: String },
    CompressRatioBad { name: String, declared: u64, compressed: u64 },
    SingleDeclaredOver { name: String, declared: u64, limit: u64 },
    TotalDeclaredOver { total: u64, limit: u64 },
    /// 实际读取超限——声明值撒谎在此截停（T-03 核心防线）。
    /// 注：返回 String 经 from_utf8_lossy，非 UTF-8 内容每字节最多膨胀 3x
    /// （U+FFFD 替换）——内存上界 3×limit，已计入防护面内（恶意文件可触发
    /// 上限 192MB 字符串，为避免拒绝真实文件的轻微编码瑕疵保留 lossy 策略）。
    ActualReadOver { name: String, read: u64, limit: u64 },
    XmlDepthOver { depth: usize, limit: usize },
    /// 中央目录声明条目数 ≠ 归档实际条目数——重复条目被 zip crate 读取层
    /// HashMap 吸收（同名只留一个）后的数量不守恒面，守恒门拒绝。
    EntryCountMismatch { declared: usize, actual: usize },
    EntryMissing { name: String },
    Io(String),
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputTooLarge { len, limit } => {
                write!(f, "输入字节超限 {len} > {limit}")
            }
            Self::EntryCountOver { count, limit } => {
                write!(f, "zip 条目数超限 {count} > {limit}")
            }
            Self::EntryNameBad { name, reason } => {
                write!(f, "异常条目名「{name}」：{reason}")
            }
            Self::DuplicateEntry { name } => write!(f, "重复条目「{name}」",),
            Self::NestedArchive { name } => {
                write!(f, "嵌套归档条目「{name}」（炸弹载具拒绝）")
            }
            Self::CompressRatioBad { name, declared, compressed } => {
                write!(f, "压缩比异常「{name}」{declared}/{compressed}（炸弹载具拒绝）")
            }
            Self::SingleDeclaredOver { name, declared, limit } => {
                write!(f, "单条目声明超限「{name}」{declared} > {limit}")
            }
            Self::TotalDeclaredOver { total, limit } => {
                write!(f, "总声明字节超限 {total} > {limit}")
            }
            Self::ActualReadOver { name, read, limit } => {
                write!(f, "实际读取超限「{name}」已读 {read} 字节 > 上限 {limit}（声明值撒谎截停）")
            }
            Self::XmlDepthOver { depth, limit } => {
                write!(f, "XML 嵌套深度超限 {depth} > {limit}")
            }
            Self::EntryCountMismatch { declared, actual } => {
                write!(f, "中央目录条目数不守恒：声明 {declared} ≠ 实际 {actual}（重复条目吸收面拒绝）")
            }
            Self::EntryMissing { name } => write!(f, "缺条目「{name}」"),
            Self::Io(s) => write!(f, "IO 错误 {s}"),
        }
    }
}

impl std::error::Error for GuardError {}

type GResult<T> = Result<T, GuardError>;

// ---------------------------------------------------------------- 输入字节门
/// 输入文件字节门（四入口单点——读任何内容之前先查 `metadata`，
/// 防"先全量读入内存做哈希/探测"型 OOM）。
pub fn check_input_size(path: &Path, limit: u64) -> GResult<()> {
    let len = std::fs::metadata(path).map_err(|e| GuardError::Io(e.to_string()))?.len();
    if len > limit {
        return Err(GuardError::InputTooLarge { len, limit });
    }
    Ok(())
}

// ---------------------------------------------------------------- prescan（声明值门）
/// prescan 报告（限内时的审计数据——不作为放行依据，仅记录）。
#[derive(Debug, Clone)]
pub struct PrescanReport {
    pub entries: usize,
    pub total_declared: u64,
    pub max_single_declared: u64,
}

/// zip 预扫描：条目数 / 单条目声明 / 总声明 / 压缩比 / 条目名五项一体裁决。
/// ⚠️ 本函数检查的是**中央目录声明值**——攻击者可控；真正的防线是
/// [`read_entry_limited`] 的流式累计计量。prescan 是快速拒绝层 + 载具甄别层。
pub fn prescan_zip(za: &mut ZipArchive<File>) -> GResult<PrescanReport> {
    let count = za.len();
    if count > LIMIT_ENTRY_COUNT {
        return Err(GuardError::EntryCountOver { count, limit: LIMIT_ENTRY_COUNT });
    }
    let mut total: u64 = 0;
    let mut max_single: u64 = 0;
    let mut names = std::collections::HashSet::new();
    for i in 0..count {
        let e = za.by_index(i).map_err(|e| GuardError::Io(e.to_string()))?;
        let name = e.name().to_string();
        let declared = e.size();
        let compressed = e.compressed_size();
        // 名字检查对目录条目同样执行——`../` 等穿越载具可借目录形态混入
        // （真实漏洞：目录条目曾绕过名字门，负向测试 deep_* 抓出后修复）
        check_entry_name(&name)?;
        if !e.is_dir() {
            if !names.insert(name.clone()) {
                return Err(GuardError::DuplicateEntry { name });
            }
            if declared > LIMIT_SINGLE_DECLARED {
                return Err(GuardError::SingleDeclaredOver { name, declared, limit: LIMIT_SINGLE_DECLARED });
            }
            // 压缩比门：compressed==0 且 declared>0 视为无穷比（stored 条目 compressed==declared）
            if declared > 0 {
                let compressed_eff = compressed.max(1);
                if declared / compressed_eff > LIMIT_COMPRESS_RATIO {
                    return Err(GuardError::CompressRatioBad { name, declared, compressed });
                }
            }
            total = total.saturating_add(declared);
            if total > LIMIT_TOTAL_DECLARED {
                return Err(GuardError::TotalDeclaredOver { total, limit: LIMIT_TOTAL_DECLARED });
            }
            max_single = max_single.max(declared);
        }
    }
    Ok(PrescanReport { entries: count, total_declared: total, max_single_declared: max_single })
}

/// 条目名检查：长度 / 穿越段 / 绝对路径 / 盘符 / 控制字符 / 路径深度 / 嵌套归档扩展名。
fn check_entry_name(name: &str) -> GResult<()> {
    let bad = |reason: &'static str| Err(GuardError::EntryNameBad { name: name.to_string(), reason });
    if name.len() > LIMIT_ENTRY_NAME_LEN {
        return bad("超长条目名");
    }
    if name.contains('\0') || name.chars().any(|c| c.is_control()) {
        return bad("含控制字符");
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return bad("绝对路径");
    }
    if name.len() >= 2 && name.as_bytes()[1] == b':' {
        return bad("盘符路径");
    }
    // 逐段穿越检查（`..` 独立成段才算；`a..b` 不是穿越）
    for seg in name.split(['/', '\\']) {
        if seg == ".." {
            return bad("路径穿越段 `..`");
        }
    }
    if name.split('/').count() > LIMIT_PATH_DEPTH {
        return bad("路径深度超限");
    }
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default(); // R3-P2-3：无点名字不得取全名当扩展名（"zip"裸名误判）
    if NESTED_ARCHIVE_EXTS.contains(&ext.as_str()) {
        return Err(GuardError::NestedArchive { name: name.to_string() });
    }
    Ok(())
}

// ---------------------------------------------------------------- 实际读取门（真防线）
/// 受限读取条目：**流式累计计量**，读出字节 > limit 即失败——
/// 与声明值无关（声明撒谎在此截停），且 `take` 保证内存占用不超过 limit+1 字节。
pub fn read_entry_limited(
    za: &mut ZipArchive<File>,
    name: &str,
    limit: u64,
) -> GResult<String> {
    let f = za
        .by_name(name)
        .map_err(|_| GuardError::EntryMissing { name: name.to_string() })?;
    let mut buf: Vec<u8> = Vec::new();
    let read = f
        .take(limit + 1)
        .read_to_end(&mut buf)
        .map_err(|e| GuardError::Io(e.to_string()))?;
    if read as u64 > limit {
        return Err(GuardError::ActualReadOver { name: name.to_string(), read: read as u64, limit });
    }
    // docx XML 契约为 UTF-8；无效字节以替换符保留（quick-xml 走 Reader::from_str）
    Ok(String::from_utf8_lossy(&buf).to_string())
}

// ---------------------------------------------------------------- 打开受检归档
/// docx/xlsx 共用入口：输入字节门 → 打开 → prescan 三门 → 条目数守恒门。
/// 返回的归档已通过声明值层检查；调用方仍必须用 [`read_entry_limited`] 读内容。
pub fn open_guarded(path: &Path, input_limit: u64) -> GResult<ZipArchive<File>> {
    check_input_size(path, input_limit)?;
    let file = File::open(path).map_err(|e| GuardError::Io(e.to_string()))?;
    let mut za = ZipArchive::new(file).map_err(|e| GuardError::Io(format!("非有效 zip/docx: {e}")))?;
    prescan_zip(&mut za)?;
    // 守恒门：EOCD 声明条目数 ≠ 归档实际条目数 → 拒绝。
    // zip crate 读取层用 HashMap 去重同名条目——重复条目进不了 prescan 视野；
    // 守恒比对封死这个吸收面（0xFFFF 为 zip64 标记位，跳过不判）。
    if let Some(declared) = eocd_declared_entry_count(path)? {
        if declared != 0xFFFF {
            let actual = za.len();
            if declared as usize != actual {
                return Err(GuardError::EntryCountMismatch { declared: declared as usize, actual });
            }
        }
    }
    Ok(za)
}

/// 解析 EOCD（End of Central Directory）的声明条目数字段。
/// 从文件尾部向前扫描「PK\x05\x06」签名（尾部 22B + 最长 65535B comment 区）；
/// 无 EOCD（zip crate 已拒绝的 malformed）返回 None，不重复裁决。
fn eocd_declared_entry_count(path: &Path) -> GResult<Option<u16>> {
    let len = std::fs::metadata(path).map_err(|e| GuardError::Io(e.to_string()))?.len();
    if len < 22 {
        return Ok(None);
    }
    let mut f = File::open(path).map_err(|e| GuardError::Io(e.to_string()))?;
    use std::io::{Seek, SeekFrom};
    let scan_start = len.saturating_sub(22 + 65_535);
    f.seek(SeekFrom::Start(scan_start)).map_err(|e| GuardError::Io(e.to_string()))?;
    let mut tail = Vec::new();
    f.read_to_end(&mut tail).map_err(|e| GuardError::Io(e.to_string()))?;
    let sig: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
    let mut found: Option<usize> = None;
    for i in (0..tail.len().saturating_sub(21)).rev() {
        if tail[i..i + 4] == sig {
            found = Some(i);
            break;
        }
    }
    match found {
        Some(i) => Ok(Some(u16::from_le_bytes(tail[i + 10..i + 12].try_into().unwrap()))),
        None => Ok(None),
    }
}

/// 流式 sha256（替换 `fs::read` 全量内存哈希——大文件面顺带根治）。
pub fn sha256_streaming(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

// ---------------------------------------------------------------- T-04 子进程治理
/// 外部命令子进程治理（S1 威胁模型 T-04 实装）：
/// - 超时 → 杀**进程组**（`process_group(0)` + `kill(-pgid, SIGKILL)`——
///   pdftoppm/tesseract 可能派生孙进程，只杀直接子进程留孤儿）；
/// - 输出上限 → stdout/stderr 各自累计计量，stdout 超限即杀即失败
///   （截断输出继续解析 = 产出损坏结果，违反「不留部分结果」）；
/// - 结果五分类：成功 / 超时 / 输出超限 / 非零退出 / 繁忙（并发满）；
///   取消、超时、崩溃**永不**返回 Success——上层据此不落 `Ok`/`EXPORTED`；
/// - 并发上限：全局原子计数，满载显式拒绝（调用方可重试），不静默排队。
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// 同时在跑的解析子进程上限（OCR 管道单文件即占 1 席；防并发洪泛）。
pub const MAX_CONCURRENT_SUBPROCS: usize = 4;
static LIVE_SUBPROCS: AtomicUsize = AtomicUsize::new(0);

/// 各外部命令的治理参数（部署清单钉住；调整需过组件版本矩阵）。
pub const TIMEOUT_PDFTOTEXT: Duration = Duration::from_secs(60);
pub const TIMEOUT_PDFTOPPM: Duration = Duration::from_secs(180);
pub const TIMEOUT_TESSERACT: Duration = Duration::from_secs(30);
pub const LIMIT_PDFTOTEXT_STDOUT: u64 = 64 * 1024 * 1024;
pub const LIMIT_TESSERACT_STDOUT: u64 = 1024 * 1024;
pub const LIMIT_SUBPROC_STDERR: u64 = 64 * 1024;

/// OCR 渲染页数上限：扫描件超页 → 整体显式拒绝（不产部分 OCR 结果）。
pub const MAX_OCR_PAGES: usize = 200;

/// 治理结果五分类（+ Spawn 失败）。Timeout/Overflow/NonZero/Busy 均为失败面，
/// 调用方**必须**向上传播——不得映射为空输出成功。
#[derive(Debug)]
pub enum RunOutcome {
    Success { stdout: String },
    Timeout { program: String, secs: u64 },
    OutputOverflow { program: String, limit: u64 },
    NonZeroExit { program: String, code: Option<i32>, stderr_head: String },
    Busy { program: String },
    Spawn { program: String, why: String },
}

impl RunOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
    /// 统一转错误字符串（pdf.rs 语义保留 program 上下文）。
    pub fn into_result(self) -> Result<String, String> {
        match self {
            Self::Success { stdout } => Ok(stdout),
            Self::Timeout { program, secs } => Err(format!("子进程超时被杀（{program} {secs}s）")),
            Self::OutputOverflow { program, limit } => {
                Err(format!("子进程输出超限（{program} > {limit} 字节）"))
            }
            Self::NonZeroExit { program, code, stderr_head } => {
                Err(format!("{program} 退出码 {code:?}: {stderr_head}"))
            }
            Self::Busy { program } => Err(format!("解析子进程并发满载（{program}），请重试")),
            Self::Spawn { program, why } => Err(format!("{program} 无法启动: {why}")),
        }
    }
}

/// 并发席 RAII：离开作用域必减计（即使 Err 提前返回）。
struct SubprocSlot;
impl SubprocSlot {
    fn acquire(program: &str) -> Result<Self, RunOutcome> {
        let n = LIVE_SUBPROCS.fetch_add(1, Ordering::SeqCst);
        if n >= MAX_CONCURRENT_SUBPROCS {
            LIVE_SUBPROCS.fetch_sub(1, Ordering::SeqCst);
            return Err(RunOutcome::Busy { program: program.to_string() });
        }
        Ok(Self)
    }
}
impl Drop for SubprocSlot {
    fn drop(&mut self) {
        LIVE_SUBPROCS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// 杀整个进程组（Unix；`pgid == child_pid` 因 spawn 时 `process_group(0)`）。
fn kill_process_tree(child: &Child) {
    #[cfg(unix)]
    {
        // negative pid = 进程组；errno 检查省略（组可能已空——kill 返回 ESRCH 属正常竞态）
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

/// 受治运行一个外部命令：超时杀组、stdout 上限、stderr 截头、五分类。
/// 溢出即时杀：读线程置共享原子标志，轮询循环立即杀组（不等超时——
/// `yes` 类无限输出进程会在管道写阻塞，必须主动收割）。
pub fn run_limited(mut cmd: Command, timeout: Duration, stdout_limit: u64) -> RunOutcome {
    use std::sync::atomic::AtomicBool;
    let program = format!("{:?}", cmd.get_program());
    let slot = match SubprocSlot::acquire(&program) {
        Ok(s) => s,
        Err(b) => return b,
    };
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // 独立进程组：孙进程可整组收割（否则孤儿进程滞留占资源——T-04 回收面）
        cmd.process_group(0);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            drop(slot);
            return RunOutcome::Spawn { program, why: e.to_string() };
        }
    };
    let started = Instant::now();

    // 双读线程：防管道写满死锁；stdout 计量超限置溢出标志（轮询循环即时杀组）。
    // R3-S2-1：结果经 channel 发送，主线程限时接收——直接子进程退出但孙进程继承
    // 持有管道时，读线程读不到 EOF，无界 join 会把 1s 超时拖成实际 60s+。
    // 限时收不到 → 补杀进程组（孙进程被杀后管道关闭、线程自然退出释放）。
    use std::sync::mpsc;
    let mut stdout_pipe = child.stdout.take().expect("stdout piped");
    let mut stderr_pipe = child.stderr.take().expect("stderr piped");
    let overflow = std::sync::Arc::new(AtomicBool::new(false));
    let ov_flag = overflow.clone();
    let out_limit = stdout_limit;
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
    let t_out = std::thread::spawn(move || {
        let mut buf: Vec<u8> = Vec::new();
        let mut limited = (&mut stdout_pipe).take(out_limit + 1);
        // 超限 take 最多读 limit+1 字节即返回——超限即置溢出标志
        let _ = std::io::Read::read_to_end(&mut limited, &mut buf);
        if buf.len() as u64 > out_limit {
            ov_flag.store(true, Ordering::SeqCst);
        }
        let _ = out_tx.send(buf);
    });
    let (err_tx, err_rx) = mpsc::channel::<String>();
    let t_err = std::thread::spawn(move || {
        // stderr 读到 EOF 为止但只保留头 64KB（R3-P2-4：截停不读会让孙进程阻塞管道）
        let mut head: Vec<u8> = Vec::new();
        let mut sink = [0u8; 4096];
        let mut limited = (&mut stderr_pipe).take(LIMIT_SUBPROC_STDERR + 1);
        let mut done = false;
        while !done {
            match std::io::Read::read(&mut limited, &mut sink) {
                Ok(0) | Err(_) => done = true,
                Ok(n) => {
                    if head.len() < LIMIT_SUBPROC_STDERR as usize {
                        let room = LIMIT_SUBPROC_STDERR as usize - head.len();
                        head.extend_from_slice(&sink[..n.min(room)]);
                    }
                }
            }
        }
        let _ = err_tx.send(String::from_utf8_lossy(&head).to_string());
    });

    // 轮询等待：超时杀组 / 溢出即时杀组（截断输出不得被当成功消费）
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if overflow.load(Ordering::SeqCst) {
                    kill_process_tree(&child);
                    let _ = child.wait();
                    return RunOutcome::OutputOverflow { program, limit: stdout_limit };
                }
                if started.elapsed() >= timeout {
                    kill_process_tree(&child);
                    let _ = child.wait();
                    return RunOutcome::Timeout { program, secs: timeout.as_secs() };
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                kill_process_tree(&child);
                let _ = child.wait();
                return RunOutcome::Spawn { program, why: format!("wait 失败: {e}") };
            }
        }
    }

    // 限时收流（R3-S2-1）：1.5s 内读不到 = 孙进程持有管道——补杀组 + 归类 Timeout
    // （线程留 detached，进程组被杀后管道关闭自然退出，无泄漏累积）。
    let stdout_bytes = match out_rx.recv_timeout(Duration::from_millis(1500)) {
        Ok(b) => b,
        Err(_) => {
            kill_process_tree(&child);
            let _ = child.wait();
            return RunOutcome::Timeout { program, secs: timeout.as_secs() };
        }
    };
    let stderr_head: String = err_rx
        .recv_timeout(Duration::from_millis(1500))
        .map(|s| s.chars().take(160).collect())
        .unwrap_or_default();
    drop(slot);

    // 竞态双检：子进程先退出但输出已溢出的场景（溢出优先——截断输出不算成功）
    if overflow.load(Ordering::SeqCst) {
        return RunOutcome::OutputOverflow { program, limit: stdout_limit };
    }
    let status = child.wait().ok();
    match status {
        Some(st) if st.success() => {
            RunOutcome::Success { stdout: String::from_utf8_lossy(&stdout_bytes).to_string() }
        }
        Some(st) => RunOutcome::NonZeroExit { program, code: st.code(), stderr_head },
        None => RunOutcome::Spawn { program, why: "无退出状态".into() },
    }
}

/// 临时目录 RAII：OCR 渲染的临时 PNG 区——超时/失败/正常路径统一回收（T-04 资源回收面）。
/// R3-S2-2：目录名 = tag + pid + 全局计数 + 纳秒——调用级唯一（并发 4 席下两个
/// 扫描件曾共享 tag+pid 目录交叉覆盖 page-*.png，静默污染 OCR 结果；崩溃残留的
/// 同名复用同样被纳秒后缀消除）。
pub struct TmpGuard(pub std::path::PathBuf);
impl TmpGuard {
    pub fn new(tag: &str) -> std::io::Result<Self> {
        static GUARD_SEQ: AtomicUsize = AtomicUsize::new(0);
        let seq = GUARD_SEQ.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!("bidparse_{tag}_{}_{seq}_{nanos}", std::process::id()));
        std::fs::create_dir_all(&p)?;
        Ok(Self(p))
    }
}
impl Drop for TmpGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------- 测试（负向为主——T-03/T-04 纪律）
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn tmp_zip(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join("bidparse_guard_test");
        std::fs::create_dir_all(&d).unwrap();
        d.join(format!("{name}_{}.zip", std::process::id()))
    }

    /// 构造恶意/边界 zip 的辅助：entries = (name, &data)。
    fn write_zip(path: &std::path::PathBuf, entries: &[(&str, Vec<u8>)]) {
        let f = File::create(path).unwrap();
        let mut w = ZipWriter::new(f);
        for (n, d) in entries {
            w.start_file(*n, SimpleFileOptions::default()).unwrap();
            w.write_all(d).unwrap();
        }
        w.finish().unwrap();
    }

    /// 高熵不可压缩字节流——**限制在 ASCII 可见字符区间**：
    /// 1) 合法 UTF-8（from_utf8_lossy 不膨胀——真实验断言用字�口径）；
    /// 2) 熵高、deflate 压不动（不触发压缩比门）。
    fn incompressible(len: usize) -> Vec<u8> {
        let mut x: u64 = 0x9E3779B97F4A7C15 | ((std::process::id() as u64) << 1) | 1;
        let mut v = Vec::with_capacity(len);
        for _ in 0..(len / 8 + 1) {
            x ^= x >> 12;
            x = x.wrapping_mul(0x2545F4914F6CDD1D);
            x ^= x >> 25;
            for b in x.to_le_bytes() {
                v.push(0x21 + (b % 0x5E)); // 0x21..0x7E 可见 ASCII
            }
        }
        v.truncate(len);
        v
    }

    /// patch 中央目录条目名：同名长度的字节替换（ZipWriter 写不出重复条目——
    /// 恶意构造面只能字节级 patch，攻击者同理）。
    fn patch_central_name(raw: &[u8], from: &str, to: &str) -> Vec<u8> {
        assert_eq!(from.len(), to.len());
        let mut out = raw.to_vec();
        let mut hit = 0;
        let mut i = 0;
        while i + 46 <= out.len() {
            if &out[i..i + 4] == b"PK\x01\x02" {
                let nlen = u16::from_le_bytes(out[i + 28..i + 30].try_into().unwrap()) as usize;
                if nlen == from.len() && &out[i + 46..i + 46 + nlen] == from.as_bytes() {
                    out[i + 46..i + 46 + nlen].copy_from_slice(to.as_bytes());
                    hit += 1;
                }
            }
            i += 1;
        }
        assert_eq!(hit, 1, "中央目录定位 {from} 失败");
        out
    }

    fn open(path: &std::path::PathBuf, limit: u64) -> GResult<ZipArchive<File>> {
        open_guarded(path, limit)
    }

    // ---- 负向：五类载具必须显式拒绝 ----

    #[test]
    fn zip_bomb_compress_ratio_rejected() {
        // 40MB 全零（压后约 40KB，比≈1000:1）→ 压缩比门拒绝
        let p = tmp_zip("ratio");
        write_zip(&p, &[("word/document.xml", vec![0u8; 40 * 1024 * 1024])]);
        let err = open(&p, LIMIT_INPUT_BYTES_DOCX).unwrap_err();
        assert!(matches!(err, GuardError::CompressRatioBad { .. }), "{err}");
    }

    #[test]
    fn entry_count_flood_rejected() {
        let p = tmp_zip("flood");
        let f = File::create(&p).unwrap();
        let mut w = ZipWriter::new(f);
        for i in 0..(LIMIT_ENTRY_COUNT + 1) {
            w.start_file(format!("f{i}.txt"), SimpleFileOptions::default()).unwrap();
            w.write_all(b"x").unwrap();
        }
        w.finish().unwrap();
        let err = open(&p, LIMIT_INPUT_BYTES_DOCX).unwrap_err();
        assert!(matches!(err, GuardError::EntryCountOver { .. }), "{err}");
    }

    #[test]
    fn path_traversal_and_bad_names_rejected() {
        let p = tmp_zip("traversal");
        write_zip(&p, &[("../evil.txt", b"x".to_vec())]);
        let err = open(&p, LIMIT_INPUT_BYTES_DOCX).unwrap_err();
        assert!(matches!(err, GuardError::EntryNameBad { reason: "路径穿越段 `..`", .. }), "{err}");

        let p = tmp_zip("abs");
        write_zip(&p, &[("/etc/passwd", b"x".to_vec())]);
        assert!(matches!(open(&p, LIMIT_INPUT_BYTES_DOCX).unwrap_err(),
            GuardError::EntryNameBad { reason: "绝对路径", .. }));

        let p = tmp_zip("ctrl");
        let name = format!("bad{}\0.txt", '\u{7}');
        write_zip(&p, &[(&name, b"x".to_vec())]);
        assert!(matches!(open(&p, LIMIT_INPUT_BYTES_DOCX).unwrap_err(),
            GuardError::EntryNameBad { reason: "含控制字符", .. }));

        let p = tmp_zip("drive");
        write_zip(&p, &[("C:\\evil.txt", b"x".to_vec())]);
        assert!(matches!(open(&p, LIMIT_INPUT_BYTES_DOCX).unwrap_err(),
            GuardError::EntryNameBad { reason: "盘符路径", .. }));

        let p = tmp_zip("deep");
        let name = format!("a/{}", vec!["b/"; 70].join(""));
        write_zip(&p, &[(&name, b"x".to_vec())]);
        assert!(matches!(open(&p, LIMIT_INPUT_BYTES_DOCX).unwrap_err(),
            GuardError::EntryNameBad { reason: "路径深度超限", .. }));
    }

    #[test]
    fn duplicate_entries_rejected_by_conservation_gate() {
        // ZipWriter 拒写重复名且 zip crate 读取层用 HashMap 吸收同名条目
        // （prescan 视野看不到重复）——字节级 patch 中央目录构造重复名后，
        // EOCD 声明 2 条 ≠ 归档实际 1 条 → 守恒门拒绝（吸收面封死）。
        let p = tmp_zip("dup");
        write_zip(&p, &[("word/aaaa.xml", b"<a/>".to_vec()), ("word/bbbb.xml", b"<b/>".to_vec())]);
        let raw = std::fs::read(&p).unwrap();
        std::fs::write(&p, patch_central_name(&raw, "word/bbbb.xml", "word/aaaa.xml")).unwrap();
        let err = open(&p, LIMIT_INPUT_BYTES_DOCX).unwrap_err();
        assert!(matches!(err, GuardError::EntryCountMismatch { declared: 2, actual: 1 }), "{err}");
    }

    #[test]
    fn nested_archive_rejected() {
        let p = tmp_zip("nested");
        write_zip(&p, &[("attach/instructions.zip", b"x".to_vec())]);
        let err = open(&p, LIMIT_INPUT_BYTES_DOCX).unwrap_err();
        assert!(matches!(err, GuardError::NestedArchive { .. }), "{err}");
    }

    #[test]
    fn input_size_gate_rejects_before_read() {
        // 4MB 不可压缩随机字节 → zip 本体 ≈4MB > 2MB 输入门在读入前拒绝
        let p = tmp_zip("big");
        write_zip(&p, &[("word/document.xml", incompressible(4 * 1024 * 1024))]);
        let err = open(&p, 2 * 1024 * 1024).unwrap_err();
        assert!(matches!(err, GuardError::InputTooLarge { .. }), "{err}");
    }

    // ---- 核心防线：声明撒谎 → 实际读取截停 ----

    /// 构造「撒谎 zip」：真实内容 8MB 高压缩，但把中央目录声明值 patch 小。
    /// 攻击者可控中央目录——prescan 会放行，[`read_entry_limited`] 必须拦下。
    #[test]
    fn lying_declaration_stopped_by_actual_read() {
        let p = tmp_zip("lying");
        let payload = "投标".repeat(4 * 1024 * 1024); // 24MB UTF-8，压后约 30KB
        write_zip(&p, &[("word/document.xml", payload.into_bytes())]);

        // patch 中央目录未压缩 size 字段为 1KB（撒谎）
        let raw = std::fs::read(&p).unwrap();
        // 定位中央目录条目：搜「PK\x01\x02」头，未压缩 size 在 +24（u32 LE）
        let mut patched = raw.clone();
        let mut i = 0;
        let mut hit = 0usize;
        while i + 46 <= patched.len() {
            if &patched[i..i + 4] == b"PK\x01\x02" {
                let n = u32::from_le_bytes(patched[i + 24..i + 28].try_into().unwrap());
                let c = u32::from_le_bytes(patched[i + 20..i + 24].try_into().unwrap());
                // 压缩比门模拟（patch 后 compressed 保持、size 变小 → 比例 <1，不触发）
                let _ = (n, c);
                patched[i + 24..i + 28].copy_from_slice(&1_000u32.to_le_bytes());
                hit += 1;
            }
            i += 1;
        }
        assert!(hit >= 1, "中央目录定位失败");
        std::fs::write(&p, patched).unwrap();

        let mut za = open(&p, LIMIT_INPUT_BYTES_DOCX).expect("撒谎声明应通过 prescan");
        // 真防线：实际读取 24MB > 上限 1KB → 拒绝
        let err = read_entry_limited(&mut za, "word/document.xml", 1_000).unwrap_err();
        assert!(matches!(err, GuardError::ActualReadOver { .. }), "{err}");
    }

    #[test]
    fn actual_read_limit_enforced_even_with_honest_declaration() {
        // 诚实声明的 2MB 不可压缩条目，读取上限 1MB → 实际读取门拒绝；
        // 受限内重读正常返回（验证 take 不破坏正常路径）
        let p = tmp_zip("honestbig");
        write_zip(&p, &[("word/document.xml", incompressible(2 * 1024 * 1024))]);
        let mut za = open(&p, LIMIT_INPUT_BYTES_DOCX).unwrap();
        let err = read_entry_limited(&mut za, "word/document.xml", 1024 * 1024).unwrap_err();
        assert!(matches!(err, GuardError::ActualReadOver { .. }), "{err}");
        let ok = read_entry_limited(&mut za, "word/document.xml", 4 * 1024 * 1024).unwrap();
        assert_eq!(ok.len(), 2 * 1024 * 1024);
    }

    // ---- 正向：正常小 zip 放行 ----

    #[test]
    fn normal_zip_passes_all_gates() {
        let p = tmp_zip("normal");
        write_zip(&p, &[
            ("word/document.xml", b"<w:document/>".to_vec()),
            ("_rels/.rels", b"<r/>".to_vec()),
            ("docProps/core.xml", b"<c/>".to_vec()),
        ]);
        let mut za = open(&p, LIMIT_INPUT_BYTES_DOCX).unwrap();
        let report = prescan_zip(&mut za).unwrap();
        assert_eq!(report.entries, 3);
        let xml = read_entry_limited(&mut za, "word/document.xml", LIMIT_ACTUAL_READ_DOCX).unwrap();
        assert_eq!(xml, "<w:document/>");
    }

    #[test]
    fn sha256_streaming_matches_full_read() {
        let p = tmp_zip("hash");
        write_zip(&p, &[("word/document.xml", b"hello world".to_vec())]);
        let h1 = sha256_streaming(&p).unwrap();
        // 与 lib.rs 原 fs::read 全量口径逐字节等价
        use sha2::{Digest, Sha256};
        let data = std::fs::read(&p).unwrap();
        let mut h = Sha256::new();
        h.update(&data);
        assert_eq!(h1, hex::encode(h.finalize()));
    }

    // ================= T-04 子进程治理 =================
    // 并行测试共享全局并发席（LIVE_SUBPROCS）——本组测试互斥串行，避免席位数竞态
    static SUBPROC_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.arg("-c").arg(script);
        c
    }

    #[test]
    fn subprocess_timeout_kills_and_classifies() {
        let _g = SUBPROC_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // sh → sleep 进程组；1.2s 超时必须杀组并分类 Timeout（不是 Success/挂死）
        let started = std::time::Instant::now();
        let out = run_limited(sh("sleep 30"), Duration::from_millis(1200), 1024 * 1024);
        assert!(matches!(out, RunOutcome::Timeout { .. }), "{out:?}");
        assert!(started.elapsed() < Duration::from_secs(5),
            "超时后 5s 内必须返回（杀组生效），实际 {:?}",
            started.elapsed());
    }

    #[test]
    fn subprocess_output_overflow_kills_fast() {
        let _g = SUBPROC_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // yes 无限输出：64KB 上限 → 溢出即时杀（不等 60s 超时——快速失败纪律）
        let started = std::time::Instant::now();
        let out = run_limited(sh("yes | head -c 100M"), Duration::from_secs(60), 64 * 1024);
        assert!(matches!(out, RunOutcome::OutputOverflow { .. }), "{out:?}");
        assert!(started.elapsed() < Duration::from_secs(10),
            "溢出必须即时收割，实际 {:?}", started.elapsed());
    }

    #[test]
    fn subprocess_nonzero_exit_classified() {
        let out = run_limited(sh("exit 3"), Duration::from_secs(10), 4096);
        match out {
            RunOutcome::NonZeroExit { code: Some(3), .. } => {}
            other => panic!("期望 NonZeroExit code=3，实际 {other:?}"),
        }
    }

    #[test]
    fn subprocess_success_roundtrip() {
        let out = run_limited(sh("printf 'hello bid'"), Duration::from_secs(10), 4096);
        match out {
            RunOutcome::Success { stdout } => assert_eq!(stdout, "hello bid"),
            other => panic!("期望 Success，实际 {other:?}"),
        }
    }

    #[test]
    fn subprocess_spawn_failure_classified() {
        let out = run_limited(Command::new("/nonexistent/binary/xyz"), Duration::from_secs(5), 4096);
        assert!(matches!(out, RunOutcome::Spawn { .. }), "{out:?}");
    }

    #[test]
    fn subprocess_busy_gate_rejects_over_concurrency() {
        let _g = SUBPROC_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // 手动占满 4 席（同文件内可访问私有 RAII）→ 第 5 个 run 必须 Busy
        let slots: Vec<SubprocSlot> = (0..MAX_CONCURRENT_SUBPROCS)
            .map(|_| SubprocSlot::acquire("test").unwrap())
            .collect();
        let out = run_limited(sh("true"), Duration::from_secs(5), 4096);
        assert!(matches!(out, RunOutcome::Busy { .. }), "{out:?}");
        drop(slots);
        // 释放后恢复
        let out2 = run_limited(sh("true"), Duration::from_secs(5), 4096);
        assert!(out2.is_success(), "释放后应恢复，实际 {out2:?}");
    }

    #[test]
    fn tmpguard_recovers_on_all_paths() {
        // 正常 drop 与提前返回路径统一回收
        let g = TmpGuard::new("t1").unwrap();
        let p = g.0.clone();
        assert!(p.exists());
        drop(g);
        assert!(!p.exists());
        // 带文件的目录
        let g2 = TmpGuard::new("t2").unwrap();
        let p2 = g2.0.clone();
        std::fs::write(g2.0.join("page-1.png"), b"png").unwrap();
        drop(g2);
        assert!(!p2.exists());
    }

    #[test]
    fn timeout_never_lies_as_success() {
        let _g = SUBPROC_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // T-04 核心纪律：超时/溢出/非零 → into_result 全部 Err（不误标成功）
        let t = run_limited(sh("sleep 5"), Duration::from_millis(300), 4096);
        assert!(t.into_result().is_err());
        let n = run_limited(sh("exit 2"), Duration::from_secs(5), 4096);
        assert!(n.into_result().is_err());
        let o = run_limited(sh("yes"), Duration::from_secs(3), 1024);
        assert!(o.into_result().is_err());
    }
}
