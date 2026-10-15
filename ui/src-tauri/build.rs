// 正式包要求管理员权限：清单里直接写 requireAdministrator，
// Windows 在进程启动前就完成提权——不会出现"先开一个普通窗口、再重启自己"的那种闪一下。
// 开发版不加，否则每次 cargo 重建都要点一次 UAC，没法调试。
//
// 清单内容就是 Tauri 默认那份（Common-Controls 6.0 依赖）+ 提权段。
const MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>"#;

fn main() {
    let mut attrs = tauri_build::Attributes::new();
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        attrs = attrs.windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(MANIFEST),
        );
    }
    tauri_build::try_build(attrs).expect("tauri-build 失败");
}
