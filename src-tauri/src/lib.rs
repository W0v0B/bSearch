// src-tauri/src/lib.rs

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use global_hotkey::{
    GlobalHotKeyManager,
    GlobalHotKeyEvent,
    hotkey::{Code, HotKey, Modifiers},
};
use tauri::{Manager, Runtime, State, Emitter, AppHandle};
use std::sync::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use walkdir::WalkDir;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use serde::{Serialize, Deserialize};
use std::process::Command;

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
fn search_apps(query: &str, app_tracker: State<'_, AppFrequencyTracker>) -> Vec<AppResult> {
    let matcher = SkimMatcherV2::default();
    // 修改 results 的类型，存储 (综合得分, AppResult)
    let mut results: Vec<(i64, AppResult)> = Vec::new();

    // 获取系统应用目录
    let app_paths = get_app_directories();
    
    println!("Searching for apps with query: {}", query);
    println!("Scanning directories: {:?}", app_paths);

    for app_dir in app_paths {
        println!("Scanning directory: {:?}", app_dir);
        
        // 遍历应用目录中的可执行文件
        for entry in WalkDir::new(app_dir)
            .follow_links(true)
            .max_depth(5) // 限制搜索深度
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| is_executable(e.path()))
        {
            let path = entry.path().to_string_lossy().to_string();
            
            // 使用文件名作为应用名称，去掉扩展名
            let file_name = entry.path().file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown App");
                
            // 处理.lnk快捷方式的情况，尝试获取目标应用名称
            let display_name = if path.to_lowercase().ends_with(".lnk") {
                // 从.lnk文件名移除可能的" - 快捷方式"后缀
                file_name.replace(" - 快捷方式", "")
            } else {
                file_name.to_string()
            };

            // 使用模糊匹配来搜索
            if let Some(score) = matcher.fuzzy_match(&display_name, query) {
                // 获取该应用的启动频率
                let frequency = {
                    // 在需要时获取锁
                    let tracker_guard = app_tracker.0.lock().unwrap();
                    *tracker_guard.get(&path).unwrap_or(&0) // 如果没有记录，频率视为 0
                }; // 锁在这里释放

                // --- 计算综合得分 ---
                const FREQUENCY_WEIGHT: i64 = 10; 
                let combined_score = score + (frequency as i64 * FREQUENCY_WEIGHT);

                println!("Found app: {} at {}", display_name, path);

                results.push((combined_score, AppResult {
                    result_type: "app".to_string(),
                    title: display_name,
                    path: path.clone(),
                    icon_path: get_app_icon(&path),
                }));
            }
        }
    }

    // 按 *综合得分* 降序排序
    results.sort_by(|a, b| b.0.cmp(&a.0));

    // 只返回匹配的应用，不包括分数
    let filtered_results: Vec<AppResult> = results.into_iter()
        .map(|(_, app)| app) // 提取 AppResult
        .take(10) // 只返回前10个结果
        .collect();
        
    println!("Found {} matching applications", filtered_results.len());
    
    filtered_results
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
        Err(e) => Err(format!("打开URL失败: {}", e))
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

            if let Some(window) = app_handle.get_webview_window("main") {
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
                eprintln!("Window with label 'main' not found.");
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
            // 只检查常见的Windows可执行文件扩展名
            return extension.eq_ignore_ascii_case("exe") || 
                   extension.eq_ignore_ascii_case("lnk"); // 包括快捷方式
        }
    }
    
    false
}

// 辅助函数: 获取应用图标路径
fn get_app_icon(app_path: &str) -> Option<String> {
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
        // 这里可以使用Windows API来从.exe或.lnk文件提取图标
        // 但这需要额外的库支持，如winapi
        
        // 简化版本可以直接返回固定图标路径
        if app_path.to_lowercase().ends_with(".lnk") {
            return Some("/windows-shortcut-icon.svg".to_string());
        } else if app_path.to_lowercase().ends_with(".exe") {
            return Some("/windows-app-icon.svg".to_string());
        }
    }
    
    // 使用默认图标
    None
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 设置全局热键
            if let Err(e) = setup_global_hotkeys(app) {
                eprintln!("设置全局热键失败: {}", e);
            }

            // 创建并显示主窗口
            let main_window = app.get_webview_window("main").unwrap();
            main_window.set_title("BSearch").unwrap();

            // 初始设置窗口隐藏，直到触发热键
            main_window.hide().unwrap();

            Ok(())
        })
        // 确保 AppFrequencyTracker 被正确管理
        .manage(AppFrequencyTracker(Mutex::new(HashMap::new())))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            search_apps, // 使用修改后的 search_apps
            launch_app,
            open_url,
            search_web,
            get_frequent_apps
        ])
        .run(tauri::generate_context!())
        .expect("运行Tauri应用程序时出错");
}