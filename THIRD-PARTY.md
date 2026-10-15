# 第三方组件

本仓库自己的代码用 CC BY-NC-SA 4.0（见 [LICENSE](LICENSE)）。下面这些不属于我们，也没法由我们授权。

## 微软 DirectX 运行库（程序补进系统的对象）

程序会把系统里缺的 DirectX 运行库 dll 复制进 `SysWOW64` / `System32`，
文件全部来自微软官方分发包 **《DirectX End-User Runtimes (June 2010)》**（`directx_Jun2010_redist.exe`），
版权归 Microsoft Corporation，按微软的可再发行条款分发。

我们不改这些二进制，只做复制。每个文件的来源、版本、签名与 SHA256 见
[docs\组件来源与校验.md](docs/组件来源与校验.md)。

**这些二进制不在本仓库里。** 它们由 `tools\stage_payload.ps1` 从官方分发包里解出来（或使用者自备到 `_work\dxredist\`）。

本工具**不包含任何 VC++ / VB / CRT 运行库**——那是另一个项目的事。

## 图标

`assets/logo.svg`、`assets/app-icon.svg` 里的拼图形状取自
[Lucide](https://lucide.dev) 的 `puzzle` 图标，ISC 许可（宽松，可自由使用）。

## 框架

程序本体是 [Tauri](https://tauri.app) 2 编译出来的，界面用 Vue 写、Vite 打。
这些代码跟着 exe 一起分发，版权归各自作者，**不受本仓库的 CC BY-NC-SA 约束**。

| 组件 | 版本 | 许可 | 版权 |
|---|---|---|---|
| Tauri（窗口 / WebView2 宿主 / IPC） | 2.12.4 | MIT **或** Apache-2.0，任选其一 | Copyright (c) 2017 - Present Tauri Apps Contributors |
| tao、wry、tauri-runtime、tauri-utils、tauri-codegen 等同一条链上的包 | 2.x | MIT **或** Apache-2.0 | 同上 |
| Vue（界面运行时） | 3.5.43 | MIT | Copyright (c) 2018-present, Yuxi (Evan) You |
| @tauri-apps/api | 2.12.3 | MIT **或** Apache-2.0 | Tauri Apps Contributors |
| Vite、@vitejs/plugin-vue（仅构建期，不进 exe） | 8.3.4 / 6.0.10 | MIT | Vite 与 Vue 团队 |

许可原文：[MIT](https://opensource.org/license/mit) · [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0)

## Rust

`ui/src-tauri/Cargo.lock` 里有 400 多个 crate，编译进 exe 的主要还有：

- `serde` / `serde_json`（MIT 或 Apache-2.0）
- `zip`（MIT）
- `windows-sys`（MIT 或 Apache-2.0，微软官方 crate）

完整依赖树：在 `ui\src-tauri` 下跑 `cargo tree -e normal`。
