## README (English Version)

# bsearch - Quick Launcher & Search Tool (Windows)

bsearch is a fast application launcher and simple web search utility for Windows, inspired by macOS Spotlight and Windows PowerToys Run. Activate it with a global hotkey to quickly find and launch applications or perform web searches. This project is built with [Tauri](https://tauri.app/) v2, using Rust for the backend and Vue.js 3 with Vite for the frontend.

[Insert Screenshot or GIF Here]
*(It's recommended to add a screenshot or GIF showcasing the main interface)*

## ✨ Features

* **Quick App Launching**: Scans Start Menu, Desktop shortcuts, and Registry entries (Uninstall, App Paths) to find installed applications.
* **Fuzzy Search**: Find applications easily by typing parts of their names.
* **Web Search Suggestions**: Provides options to search Google or Bing for any entered text.
* **Global Hotkey**: Activate the search window from anywhere using `Shift + Space`.
* **Icon Display**: Automatically extracts and displays application icons.
* **Search History**: Shows recently used search terms.
* **Frequent Apps**: Tracks and displays your most frequently launched applications.
* **Keyboard Navigation**: Full keyboard support with `↑`, `↓`, `Enter`, and `Esc`.
* **Cross-Platform Tech**: Built with Tauri, Rust, and web technologies.
* **Windows Specialized**: Optimized for Windows, utilizing WinAPI for icon extraction and shortcut parsing.

## 🛠️ Development

### Prerequisites

* **Rust Environment**: Install Rust and Cargo. Visit [rust-lang.org](https://www.rust-lang.org/tools/install).
* **Node.js**: Install Node.js (which includes npm) or yarn. Visit [nodejs.org](https://nodejs.org/).
* **Windows**:
    * WebView2 Runtime (usually included in modern Windows versions, install if missing).
    * Microsoft Visual Studio C++ Build Tools (Select the "Desktop development with C++" workload during installation).