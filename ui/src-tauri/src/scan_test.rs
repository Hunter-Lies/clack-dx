// 判定规则的单测：哪些算损坏、哪些算旧版、哪些必须放着别动。
//
// 跑法：cargo test --manifest-path ui\src-tauri\Cargo.toml -- --nocapture
#[cfg(all(test, windows))]
mod tests {
    use crate::{classify, file_version, St};
    use std::path::Path;

    const VER: [u32; 4] = [9, 29, 952, 3111];

    #[test]
    fn classify_rules() {
        // 系统里没有 -> 缺失（由 inspect 判，这里只保证"没有目标版本"算损坏）
        assert_eq!(classify(Some(VER), 100, None, 0), St::Damaged);

        // 版本一模一样、大小一样 -> 好
        assert_eq!(classify(Some(VER), 100, Some(VER), 100), St::Ok);

        // 版本比 payload 旧 -> 异常（旧版），要覆盖
        assert_eq!(
            classify(Some(VER), 100, Some([9, 18, 904, 15]), 100),
            St::Older
        );

        // 版本比 payload 新 -> 放着别动（微软安全更新过的就是这个情况）
        assert_eq!(
            classify(Some(VER), 100, Some([9, 29, 952, 4000]), 100),
            St::Ok
        );

        // 版本对但大小不对 -> 被截断
        assert_eq!(classify(Some(VER), 100, Some(VER), 50), St::Damaged);

        // 版本资源里是 0.0.0.0 的，读出来是 None -> 损坏
        assert_eq!(classify(Some(VER), 100, None, 100), St::Damaged);

        // payload 自己没版本号时退化成比大小
        assert_eq!(classify(None, 100, None, 100), St::Ok);
        assert_eq!(classify(None, 100, None, 99), St::Damaged);
    }

    #[test]
    fn classifies_real_files() {
        // 拿 payload 里真的一份做样本
        let payload = Path::new(r"..\..\_work\payload\x86\d3dx9_43.dll");
        if !payload.is_file() {
            println!("跳过：找不到 payload 样本 {payload:?}");
            return;
        }

        let ver = file_version(payload).expect("payload 里的 dll 应该读得出版本");
        println!("样本版本 = {ver:?}");
        let size = std::fs::metadata(payload).unwrap().len();

        let dir = std::env::temp_dir().join("clackdx-classify-test");
        std::fs::create_dir_all(&dir).unwrap();

        // 原样拷贝 -> 好
        let good = dir.join("good.dll");
        std::fs::copy(payload, &good).unwrap();
        assert_eq!(classify(Some(ver), size, file_version(&good), 0), St::Damaged); // 大小传 0 只是证明大小要参与判定
        assert_eq!(
            classify(Some(ver), size, file_version(&good), std::fs::metadata(&good).unwrap().len()),
            St::Ok
        );

        // 0 字节 -> 损坏
        let zero = dir.join("zero.dll");
        std::fs::write(&zero, b"").unwrap();
        assert_eq!(file_version(&zero), None);
        assert_eq!(classify(Some(ver), size, None, 0), St::Damaged);

        // 截一半 -> 损坏（读不出版本）
        let half = dir.join("half.dll");
        let bytes = std::fs::read(payload).unwrap();
        std::fs::write(&half, &bytes[..bytes.len() / 2]).unwrap();
        let half_size = std::fs::metadata(&half).unwrap().len();
        assert_eq!(
            classify(Some(ver), size, file_version(&half), half_size),
            St::Damaged
        );

        // 不存在的文件读出来是 None
        assert_eq!(file_version(&dir.join("nope.dll")), None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
