// src-tauri/src/lib.rs

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use global_hotkey::{
    GlobalHotKeyManager,
    GlobalHotKeyEvent,
    hotkey::{Code, HotKey, Modifiers},
};
use tauri::{Manager, Runtime, Window, State, Emitter, AppHandle};
use std::sync::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use walkdir::WalkDir;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use serde::{Serialize, Deserialize};
use std::process::Command;
use tauri_plugin_shell::ShellExt;

// 定义应用频率跟踪器
struct AppFrequencyTracker(Mutex<HashMap<String, u32>>);

// 定义搜索结果类型
#[derive(Serialize, Deserialize, Clone)]
struct AppResult {
    #[serde(rename = "type")]
    result_type: String,
    title: String,
    path: String,
    icon_path: Option<String>,
}

// 应用启动历史记录
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// 搜索应用的命令
#[tauri::command]
fn search_apps(query: &str, _app_tracker: State<'_, AppFrequencyTracker>) -> Vec<AppResult> {
    let matcher = SkimMatcherV2::default();
    let mut results = Vec::new();
    
    // 获取系统应用目录
    let app_paths = get_app_directories();
    
    for app_dir in app_paths {
        // 遍历应用目录中的可执行文件
        for entry in WalkDir::new(app_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| is_executable(e.path()))
        {
            let path = entry.path().to_string_lossy().to_string();
            let file_name = entry.path().file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown App");
            
            // 使用模糊匹配来搜索
            if let Some(score) = matcher.fuzzy_match(file_name, query) {
                results.push((score, AppResult {
                    result_type: "app".to_string(),
                    title: file_name.to_string(),
                    path: path.clone(),
                    icon_path: get_app_icon(&path),
                }));
            }
        }
    }
    
    // 按匹配分数排序
    results.sort_by(|a, b| b.0.cmp(&a.0));
    
    // 只返回匹配的应用，不包括分数
    results.into_iter()
        .map(|(_, app)| app)
        .take(10) // 只返回前10个结果
        .collect()
}

// 启动应用的命令
#[tauri::command]
fn launch_app(app_path: &str, app_tracker: State<'_, AppFrequencyTracker>) -> Result<(), String> {
    // 增加应用使用频率计数
    {
        let mut tracker = app_tracker.0.lock().unwrap();
        *tracker.entry(app_path.to_string()).or_insert(0) += 1;
    }
    
    // 启动应用
    match Command::new(app_path).spawn() {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("启动应用失败: {}", e))
    }
}

// 打开URL的命令
#[tauri::command]
async fn open_url(url: &str) -> Result<(), String> {
    match tauri_plugin_opener::open_url(url, Option::<&str>::None) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("打开URL失败: {}", e.to_string())) // 使用 e.to_string() 获取错误信息
    }
}

// 执行网络搜索
#[tauri::command]
fn search_web(query: &str) -> Result<(), String> {
    let search_url = format!("https://www.google.com/search?q={}", query);
    
    match open::that(&search_url) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("执行网络搜索失败: {}", e))
    }
}

// 获取常用应用
#[tauri::command]
fn get_frequent_apps(app_tracker: State<'_, AppFrequencyTracker>) -> Vec<AppResult> {
    let tracker = app_tracker.0.lock().unwrap();
    
    // 将HashMap转换为Vec并按使用频率排序
    let mut apps: Vec<(String, u32)> = tracker.clone().into_iter().collect();
    apps.sort_by(|a, b| b.1.cmp(&a.1));
    
    // 将排序后的路径转换为AppResult
    apps.into_iter()
        .take(6) // 返回前6个常用应用
        .map(|(path, _)| {
            let file_name = PathBuf::from(&path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown App")
                .to_string();
            
            AppResult {
                result_type: "app".to_string(),
                title: file_name,
                path,
                icon_path: None,
            }
        })
        .collect()
}

// 设置全局热键的处理函数
fn setup_global_hotkeys<R: Runtime>(app: &tauri::App<R>) -> Result<(), Box<dyn std::error::Error>> {
    let app_handle: AppHandle<R> = app.app_handle().clone();

    let hotkey_manager = GlobalHotKeyManager::new()?;
    let hotkey = HotKey::new(Some(Modifiers::SHIFT), Code::Space);
    hotkey_manager.register(hotkey)?;
    println!("Hotkey registered: Shift+Space (id: {})", hotkey.id());

    let receiver = GlobalHotKeyEvent::receiver();

    std::thread::spawn(move || {
        println!("Hotkey listener thread started. Waiting for events...");
        for event in receiver.iter() {
            println!("Hotkey event received: id={}", event.id);

            if let Some(window) = app_handle.get_webview_window("theUniqueLabel") {
                match window.is_visible() {
                    Ok(visible) => {
                        if visible {
                            println!("Hiding window...");
                            let _ = window.hide();
                            if let Err(e) = window.emit("window-hidden", ()) {
                                eprintln!("Failed to emit window-hidden event: {}", e);
                            }
                        } else {
                            println!("Showing window...");
                            let _ = window.show();
                            let _ = window.set_focus();
                            if let Err(e) = window.emit("window-shown", ()) {
                                eprintln!("Failed to emit window-shown event: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to check window visibility: {}", e);
                    }
                }
            } else {
                eprintln!("Window with label 'theUniqueLabel' not found.");
            }
        }
        println!("Hotkey listener thread finished.");
    });

    app.manage(hotkey_manager);

    Ok(())
}

// 辅助函数: 获取应用目录
fn get_app_directories() -> Vec<PathBuf> {
    let mut app_dirs = Vec::new();
    
    // 根据操作系统添加不同的应用目录
    #[cfg(target_os = "windows")]
    {
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            app_dirs.push(PathBuf::from(program_files));
        }
        if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
            app_dirs.push(PathBuf::from(program_files_x86));
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        app_dirs.push(PathBuf::from("/Applications"));
        
        // 也可以添加用户应用目录
        if let Some(home) = dirs::home_dir() {
            app_dirs.push(home.join("Applications"));
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        app_dirs.push(PathBuf::from("/usr/bin"));
        app_dirs.push(PathBuf::from("/usr/local/bin"));
        
        // 添加用户可执行文件目录
        if let Some(home) = dirs::home_dir() {
            app_dirs.push(home.join(".local/bin"));
        }
    }
    
    app_dirs
}

// 辅助函数: 检查文件是否是可执行文件
fn is_executable(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            return metadata.permissions().mode() & 0o111 != 0;
        }
    }
    
    #[cfg(windows)]
    {
        if let Some(extension) = path.extension() {
            return extension == "exe" || extension == "bat" || extension == "cmd";
        }
    }
    
    false
}

// 辅助函数: 获取应用图标路径
fn get_app_icon(_app_path: &str) -> Option<String> {
    // 这个函数实现可能比较复杂，需要根据操作系统使用不同的API
    // 这里提供一个简化的实现，实际应用中可能需要更复杂的逻辑
    
    #[cfg(target_os = "macos")]
    {
        // 在macOS上，应用图标通常在.app包内的Resources目录中
        let path = PathBuf::from(app_path);
        if path.extension().map_or(false, |ext| ext == "app") {
            let icon_path = path.join("Contents/Resources/AppIcon.icns");
            if icon_path.exists() {
                return icon_path.to_str().map(String::from);
            }
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        // 在Windows上，可以从.exe文件中提取图标
        // 但这需要额外的Windows API调用，这里省略
    }
    
    // 使用默认图标
    None
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if let Err(e) = setup_global_hotkeys(app) {
                eprintln!("设置全局热键失败: {}", e);
            }
            
            // 创建并显示主窗口
            let main_window = app.get_webview_window("theUniqueLabel").unwrap();
            main_window.set_title("BSearch").unwrap();
            
            // 初始设置窗口隐藏，直到触发热键
            main_window.hide().unwrap();
            
            Ok(())
        })
        .manage(AppFrequencyTracker(Mutex::new(HashMap::new())))
        .plugin(tauri_plugin_shell::init()) // 确认 shell 插件初始化方式，如果默认则可能不需要
        .invoke_handler(tauri::generate_handler![
            greet,
            search_apps,
            launch_app,
            open_url,
            search_web,
            get_frequent_apps
        ])
        .run(tauri::generate_context!())
        .expect("运行Tauri应用程序时出错");
}