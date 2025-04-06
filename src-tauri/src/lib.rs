// src-tauri/src/lib.rs

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use std::env;
use std::sync::Mutex;
use std::process::Command;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use fuzzy_matcher::FuzzyMatcher;
use winreg::{HKEY, RegKey};
use winreg::enums::*;
use global_hotkey::{GlobalHotKeyManager, GlobalHotKeyEvent, hotkey::{Code, HotKey, Modifiers}};
use tauri::{Manager, Runtime, State, Emitter, AppHandle};
use walkdir::WalkDir;
use serde::{Serialize, Deserialize};

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
// 应用数据结构
#[derive(Clone, Debug, serde::Serialize)]
pub struct AppInfo {
    name: String,
    path: String,
    icon_path: Option<String>,
    is_shortcut: bool,
}

// 获取Windows特殊文件夹路径
fn get_special_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    
    // 用户桌面
    if let Ok(userprofile) = env::var("USERPROFILE") {
        folders.push(PathBuf::from(userprofile).join("Desktop"));
    }
    
    // 公共桌面
    if let Ok(public) = env::var("PUBLIC") {
        folders.push(PathBuf::from(public).join("Desktop"));
    }
    
    // 用户开始菜单
    if let Ok(appdata) = env::var("APPDATA") {
        folders.push(PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    
    // 公共开始菜单
    if let Ok(programdata) = env::var("ProgramData") {
        folders.push(PathBuf::from(programdata).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    
    folders
}

// 在目录中查找可执行文件
fn find_executables_in_dir(dir: &Path, app_name: &str, apps: &mut Vec<AppInfo>) {
    for entry in WalkDir::new(dir)
        .max_depth(2) // 限制搜索深度
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "exe")) {
        
        apps.push(AppInfo {
            name: app_name.to_string(),
            path: entry.path().to_string_lossy().to_string(),
            icon_path: None,
            is_shortcut: false,
        });
        
        // 通常只需要找到第一个可执行文件
        break;
    }
}

// 从特殊文件夹获取快捷方式
fn get_shortcuts_from_special_folders() -> Vec<AppInfo> {
    let mut shortcuts = Vec::new();
    let special_folders = get_special_folders();
    
    for folder in special_folders {
        if !folder.exists() {
            continue;
        }
        
        for entry in WalkDir::new(folder)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext.to_string_lossy().to_lowercase() == "lnk")) {
            
            let path = entry.path();
            let shortcut_path = path.to_string_lossy().to_string();
            
            // 使用windows-rs库解析快捷方式
            if let Some(target_path) = resolve_shortcut(&shortcut_path) {
                let name = path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
                
                shortcuts.push(AppInfo {
                    name,
                    path: target_path,
                    icon_path: None,
                    is_shortcut: true,
                });
            }
        }
    }
    
    shortcuts
}

// 使用windows-rs库解析.lnk快捷方式
fn resolve_shortcut(shortcut_path: &str) -> Option<String> {
    use windows::Win32::System::Com::{IPersistFile, STGM, CoCreateInstance, CoInitialize, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{PCWSTR, Interface};
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    unsafe {
        // 初始化COM
        let _ = CoInitialize(None);

        // 创建ShellLink对象
        let shell_link: IShellLinkW = CoCreateInstance(
            &ShellLink,
            None,
            CLSCTX_INPROC_SERVER
        ).ok()?;

        // 获取IPersistFile接口
        let persist_file: IPersistFile = shell_link.cast().ok()?;
        
        // 将shortcut_path转换为宽字符字符串
        let wide_path: Vec<u16> = shortcut_path.encode_utf16().chain(std::iter::once(0)).collect();
        
        // 加载快捷方式文件
        persist_file.Load(PCWSTR(wide_path.as_ptr()), STGM::default()).ok()?;

        // 获取目标路径
        let mut buffer = [0u16; 260]; // MAX_PATH
        let mut fd = windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW::default();
        
        shell_link.GetPath(
            &mut buffer,
            &mut fd,
            0
        ).ok()?;
        
        // 将宽字符字符串转换为Rust字符串
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let os_string = OsString::from_wide(&buffer[0..len]);
        
        os_string.into_string().ok()
    }
}

// 从Uninstall注册表键获取应用
fn get_uninstall_apps(hkey: HKEY, apps: &mut Vec<AppInfo>) {
    if let Ok(uninstall) = RegKey::predef(hkey)
        .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall") {
        
        for key_result in uninstall.enum_keys().flatten() {
            if let Ok(app_key) = uninstall.open_subkey(&key_result) {
                // 只处理有DisplayName和InstallLocation的项
                if let (Ok(name), Ok(location)) = (
                    app_key.get_value::<String, _>("DisplayName"),
                    app_key.get_value::<String, _>("InstallLocation")
                ) {
                    // 查找安装目录中的主程序
                    if !location.is_empty() {
                        let location_path = PathBuf::from(&location);
                        find_executables_in_dir(&location_path, &name, apps);
                    }
                }
            }
        }
    }
}

// 从注册表获取已安装应用
fn get_installed_apps_from_registry() -> Vec<AppInfo> {
    let mut apps = Vec::new();
    
    // 获取App Paths注册表项
    if let Ok(app_paths) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths") {
        
        for key_result in app_paths.enum_keys().flatten() {
            if let Ok(app_key) = app_paths.open_subkey(&key_result) {
                if let Ok(path) = app_key.get_value::<String, _>("") {
                    let name = Path::new(&key_result)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&key_result)
                        .to_string();
                    
                    apps.push(AppInfo {
                        name,
                        path,
                        icon_path: None,
                        is_shortcut: false,
                    });
                }
            }
        }
    }
    
    // 获取已安装应用注册表项（系统范围）
    get_uninstall_apps(HKEY_LOCAL_MACHINE, &mut apps);
    
    // 获取已安装应用注册表项（用户范围）
    get_uninstall_apps(HKEY_CURRENT_USER, &mut apps);
    
    apps
}

// 搜索应用的函数
pub fn search_windows_apps(query: &str) -> Vec<AppInfo> {
    let mut all_apps = Vec::new();
    
    // 获取特殊文件夹中的快捷方式
    let shortcuts = get_shortcuts_from_special_folders();
    all_apps.extend(shortcuts);
    
    // 获取注册表中的应用
    let registry_apps = get_installed_apps_from_registry();
    all_apps.extend(registry_apps);
    
    // 如果没有查询字符串，返回所有应用
    if query.is_empty() {
        return all_apps;
    }
    
    // 按名称进行模糊匹配
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    let mut matched_apps: Vec<(i64, AppInfo)> = Vec::new();
    
    for app in all_apps {
        if let Some(score) = matcher.fuzzy_match(&app.name.to_lowercase(), &query.to_lowercase()) {
            matched_apps.push((score, app));
        }
    }
    
    // 按匹配分数排序
    matched_apps.sort_by(|a, b| b.0.cmp(&a.0));
    
    // 只返回匹配的应用，不包括分数
    matched_apps.into_iter()
        .map(|(_, app)| app)
        .collect()
}

// 搜索应用的命令
#[tauri::command]
fn search_apps(query: &str, app_tracker: State<'_, AppFrequencyTracker>) -> Vec<AppResult> {
    // 使用推荐的Windows应用搜索函数
    let windows_apps = search_windows_apps(query);
    
    // 将结果转换为AppResult格式
    let mut results: Vec<(i64, AppResult)> = Vec::new();
    
    for app in windows_apps {
        // 获取该应用的启动频率
        let frequency = {
            let tracker_guard = app_tracker.0.lock().unwrap();
            *tracker_guard.get(&app.path).unwrap_or(&0)
        };
        
        const FREQUENCY_WEIGHT: i64 = 10;
        let score = if app.is_shortcut { 100 } else { 50 }; // 快捷方式优先显示
        let combined_score = score + (frequency as i64 * FREQUENCY_WEIGHT);
        
        results.push((combined_score, AppResult {
            result_type: "app".to_string(),
            title: app.name,
            path: app.path,
            icon_path: app.icon_path,
        }));
    }
    
    // 按综合得分降序排序
    results.sort_by(|a, b| b.0.cmp(&a.0));
    
    // 只返回匹配的应用，不包括分数
    results.into_iter()
        .map(|(_, app)| app)
        .take(10)
        .collect()
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
            search_apps,
            get_frequent_apps,
            launch_app,
            open_url,
            search_web
        ])
        .run(tauri::generate_context!())
        .expect("运行Tauri应用程序时出错");
}