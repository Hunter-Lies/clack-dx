// 咔哒 DX / Clack DX —— DirectX 运行库修复工具的后端。
//
// 干的事和别人家"DX 修复工具"一样，但只补 DirectX 那几个 dll，不碰 VC++ 运行库
// ——那些工具自带的运行库是 14.29 之类的老版本，顺着点一遍"修复"反而把新版顶掉。
//
// 三个动作：数出系统缺哪些 DX 文件、把缺的从 payload 复制进系统目录、把进度推给前端。
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager};

/// 复制进行中。这段时间不允许关窗口。
static BUSY: AtomicBool = AtomicBool::new(false);

/// 正在用管理员身份重新拉起自己。这时候别清 payload——下一个进程还要用。
static RELAUNCHING: AtomicBool = AtomicBool::new(false);

/// 开了 `--verify-hash` 时，除了版本还会逐字节比对（慢，抓静默损坏）
static VERIFY_HASH: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Arch {
    X86,
    X64,
}

impl Arch {
    fn dir(self) -> &'static str {
        match self {
            Arch::X86 => "x86",
            Arch::X64 => "x64",
        }
    }
}

/// 界面上的一行。文件按名字前缀归组——payload 里的文件名就是微软原始命名，
/// 所以"这一组有哪些文件"是运行时从 payload 目录里数出来的，不用在代码里列 184 个名字。
struct Group {
    id: &'static str,
    name: &'static str,
    prefixes: &'static [&'static str],
}

const GROUPS: &[Group] = &[
    Group {
        id: "dx9",
        name: "DirectX 9 运行库",
        prefixes: &["d3dx9_"],
    },
    Group {
        id: "dx10",
        name: "DirectX 10 运行库",
        prefixes: &["d3dx10"],
    },
    Group {
        id: "dx11",
        name: "DirectX 11 运行库",
        prefixes: &["d3dx11"],
    },
    Group {
        id: "shader",
        name: "着色器编译器 D3DCompiler",
        prefixes: &["d3dcompiler_", "d3dcsx_"],
    },
    Group {
        id: "audio",
        name: "XAudio / XACT 音频",
        prefixes: &["xaudio2_", "xactengine", "xapofx", "x3daudio"],
    },
    Group {
        id: "input",
        name: "XInput 手柄支持",
        prefixes: &["xinput"],
    },
];

fn group_of(file: &str) -> Option<&'static str> {
    let lower = file.to_ascii_lowercase();
    GROUPS
        .iter()
        .find(|g| g.prefixes.iter().any(|p| lower.starts_with(p)))
        .map(|g| g.id)
}

fn group_by_id(id: &str) -> Option<&'static Group> {
    GROUPS.iter().find(|g| g.id == id)
}

#[derive(Serialize)]
pub struct GroupInfo {
    id: String,
    name: String,
    /// 界面第二列：缺 6 个 · 异常 2 个 / 已齐全
    note: String,
    total: usize,
    missing: usize,
    /// 存在但不对（截断、读不出、旧版、被顶替）的个数
    abnormal: usize,
}

/// 明细面板用：一个文件在哪些架构下不对，以及为什么
#[derive(Serialize)]
pub struct FileInfo {
    name: String,
    x86: bool,
    x64: bool,
    /// 只对异常项写原因：0 字节 / 版本 x < y / 读不出文件版本 / 内容不符
    why: String,
}

/// 一个文件在系统里的状况
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum St {
    /// 系统目录里没有
    Missing,
    /// 在，但读不出/被截断/被顶替
    Damaged,
    /// 在，但比 payload 里的旧
    Older,
    /// 在，且版本不低于 payload
    Ok,
}

/// 一个文件在 payload 里的样子 + 它在系统里的样子
struct Case {
    arch: Arch,
    name: String,
    st: St,
    /// 异常时的原因；Ok/缺失时为空
    why: String,
}

#[derive(Clone, Serialize)]
struct Progress {
    /// 界面上那一行的 id
    row: String,
    state: &'static str,
    index: usize,
    total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Clone, Serialize)]
struct Done {
    ok: bool,
    copied: usize,
    /// 覆盖回去的异常文件（损坏 / 旧版）
    repaired: usize,
    skipped: usize,
    /// 这些文件当时被占用、杀不掉占用进程，已经安排"重启后替换"
    pending_reboot: usize,
}

/// 检测时往界面推的一行：像在跑代码那样一行行往外吐
#[derive(Clone, Serialize)]
struct ScanLog {
    /// 这一行属于哪一组（换组时界面会打一行组名）
    group: String,
    text: String,
    /// ok | missing | bad
    state: &'static str,
    index: usize,
    total: usize,
}

// ---------- 路径 ----------

/// 正式版把整个 payload 以 zip 内嵌进 exe，这样一个文件就能发出去。
/// 开发版不嵌，直接读项目里那份活的 payload，改完立刻生效。
#[cfg(not(debug_assertions))]
const PAYLOAD_ZIP: &[u8] = include_bytes!("../payload.zip");

/// 同时打到控制台（开发时看）和 %TEMP%\ClackDX.log（用户出问题直接把文件发过来）
fn log(line: &str) {
    // 别用 println!：正式版没有控制台，stdout 是空句柄，写失败会 panic，
    // 而 release 配了 panic = "abort"，一个日志就能把整个程序带走。
    let _ = writeln!(std::io::stdout(), "[clackdx] {line}");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("ClackDX.log"))
    {
        let _ = writeln!(f, "[clackdx] {line}");
    }
}

/// 把内嵌的 payload 解到 %ProgramData%\ClackDX\payload\<版本>\，同一版本只解一次。
#[cfg(not(debug_assertions))]
fn ensure_payload() -> Option<PathBuf> {
    let base = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\ProgramData"))
        .join("ClackDX")
        .join("payload");
    let dir = base.join(env!("CARGO_PKG_VERSION"));
    if dir.join(".ok").is_file() {
        return Some(dir);
    }
    log(&format!("释放内置运行库到 {}", dir.display()));
    std::fs::create_dir_all(&dir).ok()?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(PAYLOAD_ZIP)).ok()?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).ok()?;
        let rel = f.enclosed_name()?.to_path_buf();
        let out = dir.join(rel);
        if f.is_dir() {
            std::fs::create_dir_all(&out).ok()?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).ok()?;
        }
        let mut w = std::fs::File::create(&out).ok()?;
        std::io::copy(&mut f, &mut w).ok()?;
    }
    std::fs::write(dir.join(".ok"), env!("CARGO_PKG_VERSION")).ok();
    Some(dir)
}

/// 退出前把解出来的 payload 删掉：那 222 MB 只在运行期间有用，
/// 留着白占地方。下次启动会重新解一遍（一两秒）。
/// 强杀进程（任务管理器）时来不及跑，那种情况会留下目录，README 里写了怎么手动删。
fn cleanup_payload() {
    if RELAUNCHING.load(Ordering::SeqCst) {
        return;
    }
    let dir = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\ProgramData"))
        .join("ClackDX")
        .join("payload");
    if dir.is_dir() {
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => log(&format!("已清理 {}", dir.display())),
            Err(e) => log(&format!("清理 {} 失败：{e}", dir.display())),
        }
    }
}

fn windir() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
}

fn is_64bit_os() -> bool {
    std::env::var_os("ProgramFiles(x86)").is_some()
}

/// 32 位文件放 SysWOW64（32 位游戏读的就是那里），64 位文件放 System32
fn target_dir(arch: Arch) -> PathBuf {
    let w = windir();
    match arch {
        Arch::X64 => w.join("System32"),
        Arch::X86 => {
            if is_64bit_os() {
                w.join("SysWOW64")
            } else {
                w.join("System32")
            }
        }
    }
}

/// 打包后从资源目录找；开发时直接指项目里的 payload（也能用 CLACKDX_PAYLOAD 覆盖）
fn payload_root(app: &AppHandle) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CLACKDX_PAYLOAD") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    #[cfg(not(debug_assertions))]
    if let Some(p) = ensure_payload() {
        return Some(p);
    }
    if let Ok(dir) = app.path().resource_dir() {
        let p = dir.join("payload");
        if p.is_dir() {
            return Some(p);
        }
    }
    for rel in [
        "../../_work/payload",
        "../../payload",
        "../payload",
        "payload",
    ] {
        let p = PathBuf::from(rel);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

// ---------- 列表 ----------

/// 这一组在 payload 里的文件：按 x86、x64 分开，各自对应一个系统目录
fn group_files(root: &Path, id: &str) -> Vec<(Arch, String)> {
    let mut out = Vec::new();
    for arch in [Arch::X86, Arch::X64] {
        let dir = root.join(arch.dir());
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.to_ascii_lowercase().ends_with(".dll") {
                continue;
            }
            if group_of(&name) == Some(id) {
                out.push((arch, name));
            }
        }
    }
    out.sort_by(|a, b| a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase()));
    out
}

// ---------- 判定：文件对不对 ----------

/// 读文件的固定版本（VS_FIXEDFILEINFO 的四段数字）。
/// 不用 FileVersion 字符串：那玩意带 `(WGGT_OCT06.Release)` 这种后缀，比不了。
#[cfg(windows)]
fn file_version(path: &Path) -> Option<[u32; 4]> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    // VS_FIXEDFILEINFO 在 windows-sys 里没导出，自己按官方布局定义
    #[repr(C)]
    struct FixedFileInfo {
        signature: u32,
        struc_version: u32,
        file_version_ms: u32,
        file_version_ls: u32,
        product_version_ms: u32,
        product_version_ls: u32,
        file_flags_mask: u32,
        file_flags: u32,
        file_os: u32,
        file_type: u32,
        file_subtype: u32,
        file_date_ms: u32,
        file_date_ls: u32,
    }

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut handle: u32 = 0;
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle);
        if size == 0 {
            return None; // 没有版本资源（0 字节、被截断、被换成别的东西都在这儿）
        }
        let mut buf = vec![0u8; size as usize];
        if GetFileVersionInfoW(wide.as_ptr(), 0, size, buf.as_mut_ptr() as *mut _) == 0 {
            return None;
        }
        let sub: Vec<u16> = "\\".encode_utf16().chain(std::iter::once(0)).collect();
        let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len: u32 = 0;
        if VerQueryValueW(
            buf.as_ptr() as *const _,
            sub.as_ptr(),
            &mut ptr,
            &mut len,
        ) == 0
            || ptr.is_null()
            || (len as usize) < std::mem::size_of::<FixedFileInfo>()
        {
            return None;
        }
        let info = &*(ptr as *const FixedFileInfo);
        if info.signature != 0xFEEF_04BD {
            return None;
        }
        let v = [
            info.file_version_ms >> 16,
            info.file_version_ms & 0xFFFF,
            info.file_version_ls >> 16,
            info.file_version_ls & 0xFFFF,
        ];
        if v == [0, 0, 0, 0] {
            return None; // 版本资源里写着 0.0.0.0，当作读不出来
        }
        Some(v)
    }
}

#[cfg(not(windows))]
fn file_version(_path: &Path) -> Option<[u32; 4]> {
    None
}

/// 判一个文件：payload 那份和系统那份各是什么样，该怎么定性。
/// 只往新了走——目标版本不低于 payload 就算好，绝不因为"内容不一样"去覆盖，
/// 免得把微软安全更新过的文件换成我们 payload 里的老版本。
fn classify(
    payload_ver: Option<[u32; 4]>,
    payload_size: u64,
    target_ver: Option<[u32; 4]>,
    target_size: u64,
) -> St {
    let Some(pv) = payload_ver else {
        // payload 自己没版本号（理论上不会有）：退化成比大小
        return if target_size == payload_size { St::Ok } else { St::Damaged };
    };
    match target_ver {
        None => St::Damaged,
        Some(tv) if tv < pv => St::Older,
        Some(tv) if tv == pv && target_size != payload_size => St::Damaged,
        Some(_) => St::Ok,
    }
}

fn fmt_ver(v: [u32; 4]) -> String {
    format!("{}.{}.{}.{}", v[0], v[1], v[2], v[3])
}

/// 扫一组：给出每个文件的状态。--verify-hash 开着时，对"版本看着没问题"的
/// 再逐字节比一遍，抓版本对、大小也对、但内容坏了的静默损坏。
fn inspect(root: &Path, id: &str) -> Vec<Case> {
    let mut out = Vec::new();
    for (arch, name) in group_files(root, id) {
        let from = root.join(arch.dir()).join(&name);
        let to = target_dir(arch).join(&name);
        let psize = std::fs::metadata(&from).map(|m| m.len()).unwrap_or(0);
        let pver = file_version(&from);

        let (st, why) = match std::fs::metadata(&to) {
            Err(_) => (St::Missing, String::new()),
            Ok(m) => {
                let tsize = m.len();
                let tver = file_version(&to);
                let st = classify(pver, psize, tver, tsize);
                let why = match st {
                    St::Damaged if tsize == 0 => "0 字节".to_string(),
                    St::Damaged if tver.is_none() => "读不出文件版本".to_string(),
                    St::Damaged => "内容不完整".to_string(),
                    St::Older => format!(
                        "版本 {} < {}",
                        tver.map(fmt_ver).unwrap_or_else(|| "?".into()),
                        pver.map(fmt_ver).unwrap_or_else(|| "?".into())
                    ),
                    _ => String::new(),
                };
                (st, why)
            }
        };

        let mut st = st;
        let mut why = why;
        if st == St::Ok && VERIFY_HASH.load(Ordering::SeqCst) && !same_bytes(&from, &to) {
            st = St::Damaged;
            why = "内容不符".to_string();
        }

        out.push(Case {
            arch,
            name,
            st,
            why,
        });
    }
    out
}

/// 逐字节比两个文件（--verify-hash 才走这条）。不走哈希：省一个依赖，
/// 而且逐字节比不哈希弱——任何差异都逃不掉。
fn same_bytes(a: &Path, b: &Path) -> bool {
    use std::io::Read;
    let (Ok(mut fa), Ok(mut fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else {
        return false;
    };
    if fa.metadata().map(|m| m.len()).ok() != fb.metadata().map(|m| m.len()).ok() {
        return false;
    }
    let mut ba = vec![0u8; 64 * 1024];
    let mut bb = vec![0u8; 64 * 1024];
    loop {
        let (na, nb) = (
            fa.read(&mut ba).unwrap_or(0),
            fb.read(&mut bb).unwrap_or(0),
        );
        if na != nb || ba[..na] != bb[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
}

// ---------- 占用 ----------

/// 谁占着这些文件？找到就把它们结束掉。
/// 用的是 Windows 自己的重启管理器——安装程序"关掉正在使用本程序的应用"就是这套 API，
/// 比自己枚举句柄稳。需要管理员权限（我们启动时就要了）。
#[cfg(windows)]
fn kill_lockers(paths: &[PathBuf]) -> Vec<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::RestartManager::{
        RmEndSession, RmForceShutdown, RmGetList, RmRegisterResources, RmShutdown, RmStartSession,
        RM_PROCESS_INFO,
    };

    let mut names: Vec<String> = Vec::new();
    let wide: Vec<Vec<u16>> = paths
        .iter()
        .map(|p| {
            p.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        })
        .collect();
    let ptrs: Vec<*const u16> = wide.iter().map(|w| w.as_ptr()).collect();

    unsafe {
        let mut session: u32 = 0;
        let mut key = [0u16; 33]; // CCH_RM_SESSION_KEY + 1
        let rc_start = RmStartSession(&mut session, 0, key.as_mut_ptr());
        if rc_start != 0 {
            return names;
        }

        let registered = RmRegisterResources(
            session,
            ptrs.len() as u32,
            ptrs.as_ptr(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
        ) == 0;
        if registered {
            let mut need: u32 = 0;
            let mut count: u32 = 0;
            let mut reasons: u32 = 0;
            // 第一遍只为问出"要几个 RM_PROCESS_INFO"（返回 ERROR_MORE_DATA）
            RmGetList(
                session,
                &mut need,
                &mut count,
                std::ptr::null_mut(),
                &mut reasons,
            );

            if need > 0 {
                let mut infos: Vec<RM_PROCESS_INFO> = Vec::with_capacity(need as usize);
                for _ in 0..need {
                    infos.push(std::mem::zeroed());
                }
                count = need;
                if RmGetList(
                    session,
                    &mut need,
                    &mut count,
                    infos.as_mut_ptr(),
                    &mut reasons,
                ) == 0
                {
                    for info in infos.iter().take(count as usize) {
                        let name: String = info
                            .strAppName
                            .iter()
                            .take_while(|c| **c != 0)
                            .map(|c| char::from_u32(*c as u32).unwrap_or('?'))
                            .collect();
                        if !name.is_empty() {
                            names.push(name);
                        }
                    }
                    // 强关：这些进程正拿着我们要写的系统 dll，不结束没法替换
                    RmShutdown(session, RmForceShutdown as u32, None);
                }
            }
        }
        RmEndSession(session);
    }

    names.sort();
    names.dedup();
    names
}

#[cfg(not(windows))]
fn kill_lockers(_paths: &[PathBuf]) -> Vec<String> {
    Vec::new()
}

/// 实在替换不了（占用它的进程杀不掉）时的最后一手：把新文件放旁边，
/// 让 Windows 在下次重启时替换掉。安装程序那句"重启后生效"就是这么来的。
#[cfg(windows)]
fn schedule_replace_on_reboot(from: &Path, to: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT};

    let staging = to.with_extension("dll.new");
    if std::fs::copy(from, &staging).is_err() {
        return false;
    }
    let wide = |p: &Path| -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let (a, b) = (wide(&staging), wide(to));
    unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), MOVEFILE_DELAY_UNTIL_REBOOT) != 0 }
}

#[cfg(not(windows))]
fn schedule_replace_on_reboot(_from: &Path, _to: &Path) -> bool {
    false
}

// ---------- 命令 ----------

/// 检测：逐个文件看系统目录里那份对不对，边查边把结果推给前端。
/// 返回的是汇总（界面报告用），中间那一行行的进度走 scan:log 事件。
#[tauri::command]
fn detect(app: AppHandle) -> Vec<GroupInfo> {
    let Some(root) = payload_root(&app) else {
        return Vec::new();
    };

    // 先全查一遍（分组顺序），顺便把每组结果留着给报告用
    let mut scanned: Vec<(&'static Group, Vec<Case>)> = Vec::new();
    for g in GROUPS {
        scanned.push((g, inspect(&root, g.id)));
    }

    // 再一行行往外吐，好算总进度
    let total: usize = scanned.iter().map(|(_, c)| c.len()).sum();
    let mut index = 0usize;
    let mut last_group = "";
    for (g, cases) in &scanned {
        for c in cases {
            let _ = app.emit(
                "scan:log",
                ScanLog {
                    group: if g.id == last_group {
                        String::new()
                    } else {
                        g.name.to_string()
                    },
                    text: format!("{}\\{}", c.arch.dir(), c.name),
                    state: match c.st {
                        St::Missing => "missing",
                        St::Damaged | St::Older => "bad",
                        St::Ok => "ok",
                    },
                    index,
                    total,
                },
            );
            index += 1;
            last_group = g.id;
            // 一行一行地吐，太快了看不出在"跑"；368 个文件 × 2ms ≈ 0.7 秒
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    let rows: Vec<GroupInfo> = scanned
        .iter()
        .map(|(g, cases)| {
            let total = cases.len();
            let missing = cases.iter().filter(|c| c.st == St::Missing).count();
            let abnormal = cases
                .iter()
                .filter(|c| matches!(c.st, St::Damaged | St::Older))
                .count();
            GroupInfo {
                id: g.id.into(),
                name: g.name.into(),
                note: note_of(missing, abnormal),
                total,
                missing,
                abnormal,
            }
        })
        .collect();

    // 检测结果进日志：用户报"它说无需修复"时，看这一行就知道它到底查到了什么
    let miss_total: usize = rows.iter().map(|r| r.missing + r.abnormal).sum();
    log(&format!(
        "检测完成：{total} 个文件，要修 {miss_total} 个（{}）",
        rows.iter()
            .filter(|r| r.missing + r.abnormal > 0)
            .map(|r| format!("{} 缺 {} 异常 {}", r.id, r.missing, r.abnormal))
            .collect::<Vec<_>>()
            .join("，")
    ));

    rows
}

/// 右侧那一列的字：缺几个 / 异常几个 / 已齐全
fn note_of(missing: usize, abnormal: usize) -> String {
    match (missing, abnormal) {
        (0, 0) => "已齐全".into(),
        // 行里右半边就那么点地方，能省的字全省了
        (0, n) => format!("异常 {n}"),
        (m, 0) => format!("缺 {m}"),
        (m, n) => format!("缺 {m} · 异常 {n}"),
    }
}

#[tauri::command]
fn install(app: AppHandle, rows: Vec<String>) {
    std::thread::spawn(move || run_repair(app, rows));
}

/// 某个分组哪些文件不对（缺失 / 异常）——界面点开那一行时用
#[tauri::command]
fn missing_files(app: AppHandle, id: String) -> Vec<FileInfo> {
    let Some(root) = payload_root(&app) else {
        return Vec::new();
    };
    let mut out: Vec<FileInfo> = Vec::new();
    for c in inspect(&root, &id) {
        if c.st == St::Ok {
            continue;
        }
        match out.iter_mut().find(|f| f.name == c.name) {
            Some(f) => match c.arch {
                Arch::X86 => f.x86 = true,
                Arch::X64 => f.x64 = true,
            },
            None => out.push(FileInfo {
                name: c.name,
                x86: c.arch == Arch::X86,
                x64: c.arch == Arch::X64,
                why: c.why,
            }),
        }
    }
    out.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    out
}

#[tauri::command]
fn is_elevated() -> bool {
    elevated()
}

/// 兜底退出：正常关窗口在极端情况下可能不生效（窗口失焦、事件没送到），
/// 那就直接结束自己。
#[tauri::command]
fn quit() {
    cleanup_payload();
    std::process::exit(0);
}

/// 用系统默认程序打开链接。界面上的两个跳转键走这里。
/// 不用 WebView 的 window.open：那个在 WebView2 里要额外配权限，还不一定出得去。
#[tauri::command]
fn open_url(url: String) -> bool {
    // 只放行 http(s)。参数来自前端，别让这个命令变成"随便启动什么"的入口
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        log(&format!("拒绝打开非 http(s) 链接：{url}"));
        return false;
    }
    log(&format!("打开链接 {url}"));

    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
        let verb = wide("open");
        let target = wide(&url);
        let ok = unsafe {
            let r = ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
            // <= 32 是失败码
            r as isize > 32
        };
        if !ok {
            log("系统没能打开这个链接");
        }
        return ok;
    }

    #[cfg(not(windows))]
    {
        false
    }
}

/// 没管理员权限就重新以管理员身份拉起自己，返回 true 表示已经重启过了。
#[tauri::command]
fn request_elevation(interactive: bool) -> bool {
    if elevated() {
        return false;
    }
    if cfg!(debug_assertions) && !interactive {
        return false;
    }
    relaunch_elevated()
}

// ---------- 修复 ----------

fn run_repair(app: AppHandle, rows: Vec<String>) {
    BUSY.store(true, Ordering::SeqCst);
    run_repair_inner(&app, rows);
    BUSY.store(false, Ordering::SeqCst);
}

fn run_repair_inner(app: &AppHandle, rows: Vec<String>) {
    let Some(root) = payload_root(app) else {
        let _ = app.emit(
            "install:done",
            Done {
                ok: false,
                copied: 0,
                repaired: 0,
                skipped: 0,
                pending_reboot: 0,
            },
        );
        log("找不到 payload 目录");
        return;
    };

    // 先算总工作量：这次要处理多少个文件（缺的 + 异常的）
    let mut plan: Vec<(&'static Group, Vec<(Arch, String, St)>)> = Vec::new();
    for id in &rows {
        let Some(g) = group_by_id(id) else { continue };
        let todo: Vec<(Arch, String, St)> = inspect(&root, g.id)
            .into_iter()
            .filter(|c| c.st != St::Ok)
            .map(|c| (c.arch, c.name, c.st))
            .collect();
        if !todo.is_empty() {
            plan.push((g, todo));
        } else {
            // 一行都不用动也要推一条，界面才会把它标成完成
            let _ = app.emit(
                "install:progress",
                Progress {
                    row: g.id.into(),
                    state: "done",
                    index: 0,
                    total: 1,
                    message: None,
                },
            );
        }
    }

    let total: usize = plan.iter().map(|(_, f)| f.len()).sum();
    let mut index = 0usize;
    let mut copied = 0usize;
    let mut repaired = 0usize;
    let mut failed = 0usize;
    let mut pending_reboot = 0usize;

    for (g, files) in &plan {
        log(&format!("开始 {} ({})", g.id, g.name));
        let mut last_error: Option<String> = None;
        let mut locked: Vec<(Arch, String, St)> = Vec::new();

        for (arch, name, st) in files {
            let from = root.join(arch.dir()).join(name);
            let to = target_dir(*arch).join(name);
            let what = format!("{}\\{}", arch.dir(), name);
            // 缺的复制、异常的覆盖。这个计划里没有 Ok 的文件——
            // 版本不低于 payload 的一律不碰，绝不降级。
            match std::fs::copy(&from, &to) {
                Ok(_) => {
                    if *st == St::Missing {
                        copied += 1;
                        log(&format!("补上 {what}"));
                    } else {
                        repaired += 1;
                        log(&format!("覆盖 {what}"));
                    }
                }
                Err(e) => {
                    log(&format!("{what} 被占用或写不进去：{e}"));
                    locked.push((*arch, name.clone(), *st));
                }
            }
            index += 1;
            // 正在补哪个文件也推给界面，状态行上滚动显示，不至于盯着一个百分比干等
            let _ = app.emit(
                "install:progress",
                Progress {
                    row: g.id.into(),
                    state: "run",
                    index,
                    total,
                    message: Some(what),
                },
            );
        }

        // 有被占用的：找出占着它的进程、结束掉、再补一遍
        if !locked.is_empty() {
            let paths: Vec<PathBuf> = locked
                .iter()
                .map(|(arch, name, _)| target_dir(*arch).join(name))
                .collect();
            let who = kill_lockers(&paths);
            if !who.is_empty() {
                let msg = format!("解除占用 {}", who.join("、"));
                log(&msg);
                let _ = app.emit(
                    "install:progress",
                    Progress {
                        row: g.id.into(),
                        state: "run",
                        index,
                        total,
                        message: Some(msg),
                    },
                );
                // 等那几个进程真的退掉、句柄放出来
                std::thread::sleep(std::time::Duration::from_millis(900));
            }

            for (arch, name, st) in locked {
                let from = root.join(arch.dir()).join(&name);
                let to = target_dir(arch).join(&name);
                match std::fs::copy(&from, &to) {
                    Ok(_) => {
                        if st == St::Missing {
                            copied += 1;
                        } else {
                            repaired += 1;
                        }
                        log(&format!("{name} 重试成功"));
                    }
                    Err(e) => {
                        // 最后一手：安排重启后替换
                        if schedule_replace_on_reboot(&from, &to) {
                            pending_reboot += 1;
                            log(&format!("{name} 仍然被占用，已安排重启后替换"));
                        } else {
                            failed += 1;
                            let msg = format!("{} 复制失败：{e}", name);
                            log(&msg);
                            if last_error.is_none() {
                                last_error = Some(msg);
                            }
                        }
                    }
                }
                index += 1;
                let _ = app.emit(
                    "install:progress",
                    Progress {
                        row: g.id.into(),
                        state: "run",
                        index,
                        total,
                        message: None,
                    },
                );
            }
        }

        let _ = app.emit(
            "install:progress",
            Progress {
                row: g.id.into(),
                state: if last_error.is_none() { "done" } else { "fail" },
                index,
                total,
                message: last_error,
            },
        );
    }

    log(&format!(
        "完成：补上 {copied} 个，覆盖 {repaired} 个，失败 {failed} 个，待重启 {pending_reboot} 个"
    ));
    let _ = app.emit(
        "install:done",
        Done {
            ok: failed == 0,
            copied,
            repaired,
            skipped: 0,
            pending_reboot,
        },
    );
}

// ---------- 权限 ----------

#[cfg(windows)]
fn elevated() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut info = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut info as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(token);
        ok != 0 && info.TokenIsElevated != 0
    }
}

/// 用 runas 重新拉起自己。返回 true 表示已经拉起来了，调用方应当退出。
#[cfg(windows)]
fn relaunch_elevated() -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let verb = wide("runas");
    let file = wide(&exe.to_string_lossy());
    let params = wide("--elevated");

    // 拉起新进程后本进程就要退场了，这时候不能清 payload：
    // 新进程还要用它（而且会自己负责退出时清理）
    RELAUNCHING.store(true, Ordering::SeqCst);
    unsafe {
        let r = ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
        r as isize > 32
    }
}

pub fn run() {
    // 暗门：--verify-hash 时额外逐字节比对（抓版本对、大小也对、内容坏了的）
    if std::env::args().any(|a| a == "--verify-hash" || a == "--verify") {
        VERIFY_HASH.store(true, Ordering::SeqCst);
        log("已开启逐字节校验模式");
    }

    tauri::Builder::default()
        .on_window_event(|_window, event| {
            // 复制中把关闭请求挡回去：Alt+F4、任务栏"关闭窗口"、我们自己的关闭键都会走到这里
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if BUSY.load(Ordering::SeqCst) {
                    api.prevent_close();
                    log("复制进行中，已忽略关闭请求");
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            detect,
            install,
            missing_files,
            is_elevated,
            request_elevation,
            open_url,
            quit
        ])
        .build(tauri::generate_context!())
        .expect("启动失败")
        .run(|_app, event| {
            // 正常关窗口也走到这儿——把解出来的 payload 收走
            if let tauri::RunEvent::Exit = event {
                cleanup_payload();
            }
        });
}

// 解除占用那段的自测（cargo test）
#[cfg(test)]
mod lock_test;

// 损坏判定那段的自测（cargo test）
#[cfg(test)]
mod scan_test;
