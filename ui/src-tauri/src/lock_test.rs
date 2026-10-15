// 验证"解除占用"这套真的管用：起一个进程把文件拿住不放，
// 看 kill_lockers 能不能把它揪出来并结束掉。
//
// 跑法：cargo test --manifest-path ui\src-tauri\Cargo.toml -- --nocapture
#[cfg(all(test, windows))]
mod tests {
    use crate::kill_lockers;
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    /// 让子进程自己不带控制台：不然 RmShutdown 杀掉它时，会连带给我们这个
    /// 控制台发 Ctrl+C（正式版没控制台，所以只有测试里要防这一手）
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    #[test]
    fn kills_the_process_holding_a_file() {
        // 1) 造一个文件，让 pwsh 把它独占打开
        let dir = std::env::temp_dir().join("clackdx-lock-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("locked.dll");
        std::fs::write(&file, b"test").unwrap();

        // 子进程锁上文件之后写个 ready 标记，测试等标记——不要靠"睡几秒应该够了"，
        // 杀毒软件扫新构建的测试二进制时，powershell 启动可能要十几秒。
        let ready = dir.join("ready.txt");
        let script = format!(
            "$s=[System.IO.File]::Open('{0}','Open','Read','None'); [System.IO.File]::WriteAllText('{1}',''); Start-Sleep -Seconds 60",
            file.display(),
            ready.display()
        );
        let errfile = dir.join("child.err");
        let mut child = Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::from(std::fs::File::create(&errfile).unwrap()))
            .spawn()
            .expect("起不来 powershell");

        // 等它真锁上（最多 60 秒，正常一两秒）
        let mut waited = 0;
        while !ready.is_file() && waited < 600 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            waited += 1;
        }
        assert!(ready.is_file(), "子进程 60 秒都没锁上文件");

        // 确认它真的锁住了：这一下复制必须失败（共享冲突）
        let conflict = std::fs::copy(&file, dir.join("copy-target.dll")).is_err();

        // 2) 这一步以前会因共享冲突失败——正是我们要模拟的现场
        println!("先确认它真被占住了：复制报错 = {conflict}");
        if !conflict {
            println!("子进程还活着吗：{}", child.try_wait().unwrap().is_none());
            println!(
                "子进程 stderr：{}",
                std::fs::read_to_string(&errfile).unwrap_or_default()
            );
        }

        // 3) 揪出占用者
        let who = kill_lockers(&[PathBuf::from(&file)]);
        println!("kill_lockers 找到：{who:?}");
        assert!(!who.is_empty(), "没找到占用文件的进程");
        assert!(
            who.iter().any(|n| n.to_lowercase().contains("powershell")),
            "找到的进程不对：{who:?}"
        );

        // 4) 那个进程应该已经没了
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let still_alive = child.try_wait().unwrap().is_none();
        println!("占用进程还活着吗：{still_alive}");
        assert!(!still_alive, "占用进程没被结束掉");

        // 5) 现在能覆盖了
        let ok = std::fs::copy(&file, dir.join("copy-target.dll")).is_ok();
        println!("再试一次能覆盖吗：{ok}");
        assert!(ok, "结束占用后还是写不进去");

        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
