# bsearch - 快捷启动与搜索工具 (Windows)

bsearch 是一个为 Windows 设计的快速应用程序启动器和简单的网络搜索工具，灵感来源于 macOS Spotlight 和 Windows PowerToys Run。它通过全局热键激活，让你能够快速查找并启动应用，或执行网络搜索。本项目使用 [Tauri](https://tauri.app/) v2 构建，后端采用 Rust，前端采用 Vue.js 3 和 Vite。

## ✨ 功能特性

* **快速应用启动**: 扫描开始菜单、桌面快捷方式和注册表项，查找已安装的应用。
* **模糊搜索**: 输入应用名称的部分字符即可进行模糊匹配查找。
* **网络搜索建议**: 输入任意文本后，提供在 Google 或 Bing 上搜索的选项。
* **全局热键**: 使用 `Shift + Space` 随时随地激活搜索窗口。
* **图标显示**: 自动提取并显示应用程序的图标。
* **历史记录**: 显示最近的搜索词条。
* **常用应用**: 自动跟踪并显示最常启动的应用。
* **键盘导航**: 完全支持使用 `↑` `↓` `Enter` `Esc` 进行键盘操作。
* **跨平台技术**: 基于 Tauri、Rust 和 Web 技术构建。
* **Windows 特化**: 针对 Windows 平台进行优化，利用 WinAPI 进行图标提取和快捷方式解析。

## 🛠️ 开发

### 先决条件

* **Rust 环境**: 安装 Rust 和 Cargo。访问 [rust-lang.org](https://www.rust-lang.org/tools/install)。
* **Node.js**: 安装 Node.js (包含 npm) 或 yarn。访问 [nodejs.org](https://nodejs.org/)。
* **Windows**:
    * WebView2 Runtime (通常随新版 Windows 自带，如没有需安装)。
    * Microsoft Visual Studio C++ Build Tools (选择 "Desktop development with C++" 工作负载)。

## 作者留言

我知道里面的代码很乱，基本借助AI完成，但是就先这样吧，激活搜索窗口时每次会闪烁一次，尝试了很多办法都没能解决 emm...