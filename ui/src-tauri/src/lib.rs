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
    Group { id: "dx9",    name: "DirectX 9 运行库",        prefixes: &["d3dx9_"] },
    Group { id: "dx10",   name: "DirectX 10 运行库",       prefixes: &["d3dx10"] },
    Group { id: "dx11",   name: "DirectX 11 运行库",       prefixes: &["d3dx11"] },
    Group { id: "shader", name: "着色器编译器 D3DCompiler", prefixes: &["d3dcompiler_", "d3dcsx_"] },
    Group { id: "audio",  name: "XAudio / XACT 音频",       prefixes: &["xaudio2_", "xactengine", "xapofx", "x3daudio"] },
    Group { id: "input",  name: "XInput 手柄支持",          prefixes: &["xinput"] },
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
    /// 界面第二列：缺几个 / 已齐全
    note: String,
    total: usize,
    missing: usize,
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
    skipped: usize,
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
    for rel in ["../../_work/payload", "../../payload", "../payload", "payload"] {
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
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
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

fn missing_of(root: &Path, id: &str) -> (usize, usize, Vec<(Arch, String)>) {
    let files = group_files(root, id);
    let missing: Vec<(Arch, String)> = files
        .iter()
        .filter(|(arch, name)| !target_dir(*arch).join(name).is_file())
        .cloned()
        .collect();
    (files.len(), missing.len(), missing)
}

// ---------- 命令 ----------

#[tauri::command]
fn scan(app: AppHandle) -> Vec<GroupInfo> {
    let Some(root) = payload_root(&app) else {
        return Vec::new();
    };
    GROUPS
        .iter()
        .map(|g| {
            let (total, missing, _) = missing_of(&root, g.id);
            GroupInfo {
                id: g.id.into(),
                name: g.name.into(),
                note: if missing == 0 {
                    "已齐全".into()
                } else {
                    format!("缺 {missing} 个")
                },
                total,
                missing,
            }
        })
        .collect()
}

#[tauri::command]
fn install(app: AppHandle, rows: Vec<String>) {
    std::thread::spawn(move || run_repair(app, rows));
}

#[tauri::command]
fn is_elevated() -> bool {
    elevated()
}

/// 兜底退出：正常关窗口在极端情况下可能不生效（窗口失焦、事件没送到），
/// 那就直接结束自己。
#[tauri::command]
fn quit() {
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
            Done { ok: false, copied: 0, skipped: 0 },
        );
        log("找不到 payload 目录");
        return;
    };

    // 先算总工作量：这次要处理多少个文件（只算缺的）
    let mut plan: Vec<(&'static Group, Vec<(Arch, String)>)> = Vec::new();
    for id in &rows {
        let Some(g) = group_by_id(id) else { continue };
        let (_, _, missing) = missing_of(&root, g.id);
        if !missing.is_empty() {
            plan.push((g, missing));
        } else {
            // 一行都不缺也要推一条，界面才会把它标成完成
            let _ = app.emit(
                "install:progress",
                Progress { row: g.id.into(), state: "done", index: 0, total: 1, message: None },
            );
        }
    }

    let total: usize = plan.iter().map(|(_, f)| f.len()).sum();
    let mut index = 0usize;
    let mut copied = 0usize;
    let mut failed = 0usize;

    for (g, files) in &plan {
        log(&format!("开始 {} ({})", g.id, g.name));
        let mut last_error: Option<String> = None;
        for (arch, name) in files {
            let from = root.join(arch.dir()).join(name);
            let to = target_dir(*arch).join(name);
            // 只补缺的：已经存在的文件一律不覆盖。
            // 覆盖正在被别的程序加载的系统 dll 会失败（共享冲突），
            // 而且这些文件是"按名字和版本共存"的，存在即正确。
            match std::fs::copy(&from, &to) {
                Ok(_) => copied += 1,
                Err(e) => {
                    failed += 1;
                    let msg = format!("{} 复制失败：{e}", name);
                    log(&msg);
                    if last_error.is_none() {
                        last_error = Some(msg);
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

    log(&format!("完成：复制 {copied} 个，失败 {failed} 个"));
    let _ = app.emit(
        "install:done",
        Done { ok: failed == 0, copied, skipped: 0 },
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
            scan,
            install,
            is_elevated,
            request_elevation,
            open_url,
            quit
        ])
        .run(tauri::generate_context!())
        .expect("启动失败");
}
