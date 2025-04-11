// src-tauri/src/lib.rs
use std::fs;
use std::env;
use std::fs::File;
use std::sync::Arc;
use std::ptr::null_mut;
use std::process::Command;
use std::sync::{Once, Mutex};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};

use winreg::enums::*;
use winreg::{HKEY, RegKey};

use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::Win32::System::Com::{CoCreateInstance, CoInitialize, CLSCTX_INPROC_SERVER};

use image::ImageOutputFormat;
use fuzzy_matcher::FuzzyMatcher;
use global_hotkey::{GlobalHotKeyManager, GlobalHotKeyEvent, hotkey::{Code, HotKey, Modifiers}};
use tauri::{Manager, Runtime, State, Emitter, AppHandle};
use walkdir::WalkDir;
use serde::{Serialize, Deserialize};
use base64::{engine::general_purpose, Engine as _};

// 添加这个静态变量来生成唯一的图标ID
static ICON_ID: AtomicU64 = AtomicU64::new(1);
static INIT: Once = Once::new();

// 定义应用频率跟踪器
struct AppFrequencyTracker(Mutex<HashMap<String, u32>>);

// 应用数据结构
#[derive(Clone, Debug, serde::Serialize)]
pub struct AppInfo {
    name: String,
    path: String,
    icon_path: Option<String>,
    is_shortcut: bool,
}

// 应用缓存结构
struct AppCache {
    apps: Arc<Mutex<Vec<AppInfo>>>,
    last_update: Arc<Mutex<std::time::Instant>>,
    is_updating: Arc<AtomicBool>,
}

// 定义搜索结果类型
#[derive(Serialize, Deserialize, Clone)]
struct AppResult {
    #[serde(rename = "type")]
    result_type: String,
    title: String,
    path: String,
    icon_path: Option<String>,
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
    use windows::Win32::System::Com::{IPersistFile, STGM};
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

// 添加此函数来初始化图标缓存目录
fn init_icon_cache() -> PathBuf {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("bsearch")
        .join("icons");

    if !cache_dir.exists() {
        std::fs::create_dir_all(&cache_dir).unwrap_or_else(|_| {
            eprintln!("Failed to create icon cache directory");
        });
    }

    cache_dir
}

// 添加提取图标的函数
fn extract_icon_from_exe(exe_path: &str) -> Option<String> {
    // 初始化图标缓存目录
    INIT.call_once(|| {
        let _ = init_icon_cache();
    });

    let icon_id = ICON_ID.fetch_add(1, Ordering::SeqCst);
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("bsearch")
        .join("icons");
    
    let icon_path = cache_dir.join(format!("icon_{}.png", icon_id));
    
    unsafe {
        // 初始化COM
        let _ = CoInitialize(None);
        
        // 创建宽字符路径
        let wide_path: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
        
        // 尝试提取图标
        let hicon = windows::Win32::UI::Shell::ExtractIconW(
            Some(windows::Win32::Foundation::HINSTANCE(null_mut())),
            PCWSTR(wide_path.as_ptr()),
            0
        );
        
        if !hicon.is_invalid() {
            // 获取DC和兼容DC
            let hdc = windows::Win32::Graphics::Gdi::GetDC(None);
            if hdc.is_invalid() {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(hicon);
                return None;
            }
            
            let hdc_mem = windows::Win32::Graphics::Gdi::CreateCompatibleDC(Some(hdc));
            if hdc_mem.is_invalid() {
                let _ = windows::Win32::Graphics::Gdi::ReleaseDC(None, hdc);
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(hicon);
                return None;
            }
            
            // 创建48x48位图
            let hbmp = windows::Win32::Graphics::Gdi::CreateCompatibleBitmap(hdc, 48, 48);
            if hbmp.is_invalid() {
                let _ = windows::Win32::Graphics::Gdi::DeleteDC(hdc_mem);
                let _ = windows::Win32::Graphics::Gdi::ReleaseDC(None, hdc);
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(hicon);
                return None;
            }
            
            // 选择位图到内存DC
            let old_obj = windows::Win32::Graphics::Gdi::SelectObject(hdc_mem, hbmp.into());
            
            // 设置背景色为透明
            let _ = windows::Win32::Graphics::Gdi::SetBkColor(hdc_mem, COLORREF(0x00FFFFFF));
            let _ = windows::Win32::Graphics::Gdi::SetBkMode(hdc_mem, windows::Win32::Graphics::Gdi::TRANSPARENT);
            
            // 绘制图标
            let _ = windows::Win32::UI::WindowsAndMessaging::DrawIconEx(
                hdc_mem,
                0, 0,
                hicon,
                48, 48,
                0,
                None,
                windows::Win32::UI::WindowsAndMessaging::DI_NORMAL
            );
            
            // 创建位图文件
            let result = save_bitmap_as_png(hbmp, &icon_path);
            
            // 清理资源
            let _ = windows::Win32::Graphics::Gdi::SelectObject(hdc_mem, old_obj);
            let _ = windows::Win32::Graphics::Gdi::DeleteObject(hbmp.into());
            let _ = windows::Win32::Graphics::Gdi::DeleteDC(hdc_mem);
            let _ = windows::Win32::Graphics::Gdi::ReleaseDC(None, hdc);
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(hicon);
            
            if result {
                return Some(icon_path.to_string_lossy().to_string());
            }
        }
    }
    
    // 如果提取失败，返回默认图标路径
    let default_icon_path = cache_dir.join("default_icon.png");
    
    // 如果默认图标不存在，创建一个简单的默认图标
    if !default_icon_path.exists() {
        create_default_icon(&default_icon_path);
    }
    
    if default_icon_path.exists() {
        Some(default_icon_path.to_string_lossy().to_string())
    } else {
        None
    }
}

// 将位图保存为PNG文件
fn save_bitmap_as_png(hbmp: windows::Win32::Graphics::Gdi::HBITMAP, path: &Path) -> bool {
    unsafe {
        // 获取位图信息
        let mut bmi: windows::Win32::Graphics::Gdi::BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = 48;
        bmi.bmiHeader.biHeight = -48; // 负值表示自上而下的DIB
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32; // 32位RGBA
        bmi.bmiHeader.biCompression = windows::Win32::Graphics::Gdi::BI_RGB.0;
        
        // 分配内存存储像素数据
        let data_size = 48 * 48 * 4;
        let mut buffer = vec![0u8; data_size];
        
        // 获取位图数据
        let dc = windows::Win32::Graphics::Gdi::GetDC(None);
        let result = windows::Win32::Graphics::Gdi::GetDIBits(
            dc,
            hbmp,
            0,
            48,
            Some(buffer.as_mut_ptr() as *mut std::ffi::c_void),
            &mut bmi,
            windows::Win32::Graphics::Gdi::DIB_RGB_COLORS
        );
        let _ = windows::Win32::Graphics::Gdi::ReleaseDC(None, dc);
        
        if result == 0 {
            return false;
        }
        
        // 创建PNG文件
        let file = match File::create(path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let writer = std::io::BufWriter::new(file);
        
        // 使用image crate将RGBA数据保存为PNG
        // 注意：需要添加image crate作为依赖
        use image::{ImageBuffer, Rgba};
        
        let mut img = ImageBuffer::<Rgba<u8>, _>::new(48, 48);
        
        // BGRA到RGBA转换
        for y in 0..48 {
            for x in 0..48 {
                let i = (y * 48 + x) * 4;
                let b = buffer[i];
                let g = buffer[i + 1];
                let r = buffer[i + 2];
                let a = buffer[i + 3];
                
                img.put_pixel(x as u32, y as u32, Rgba([r, g, b, a]));
            }
        }
        
        img.write_to(&mut writer.into_inner().unwrap(), ImageOutputFormat::Png).is_ok()
    }
}

// 创建默认图标
fn create_default_icon(path: &Path) -> bool {
    use image::{ImageBuffer, Rgba};
    
    // 创建一个48x48的简单图标
    let mut img = ImageBuffer::<Rgba<u8>, _>::new(48, 48);
    
    // 绘制一个简单的应用图标
    for y in 0..48 {
        for x in 0..48 {
            let dx = x as i32 - 24;
            let dy = y as i32 - 24;
            let color = if dx * dx + dy * dy < 20 * 20 {
                Rgba([100, 149, 237, 255]) // 淡蓝色
            } else {
                Rgba([0, 0, 0, 0])
            };
    
            img.put_pixel(x, y, color);
        }
    }
    
    
    // 保存图像
    img.save(path).is_ok()
}

#[tauri::command]
async fn get_icon_data(path: String) -> Result<String, String> {
    match fs::read(&path) {
        Ok(data) => {
            let base64 = general_purpose::STANDARD.encode(&data);
            let mime = mime_guess::from_path(&path)
                .first_or_octet_stream()
                .to_string();
            Ok(format!("data:{};base64,{}", mime, base64))
        },
        Err(e) => Err(e.to_string()),
    }
}

// 搜索应用的函数
// 修改 search_windows_apps 函数，在返回结果前提取图标
pub fn search_windows_apps(query: &str) -> Vec<AppInfo> {
    let mut all_apps = Vec::new();
    
    // 获取特殊文件夹中的快捷方式
    let mut shortcuts = get_shortcuts_from_special_folders();
    
    // 为每个快捷方式提取图标
    for app in &mut shortcuts {
        if app.icon_path.is_none() {
            app.icon_path = extract_icon_from_exe(&app.path);
        }
    }
    all_apps.extend(shortcuts);
    
    // 获取注册表中的应用
    let mut registry_apps = get_installed_apps_from_registry();
    
    // 为每个注册表应用提取图标
    for app in &mut registry_apps {
        if app.icon_path.is_none() {
            app.icon_path = extract_icon_from_exe(&app.path);
        }
    }
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
fn search_apps(query: &str, app_cache: State<'_, AppCache>, app_tracker: State<'_, AppFrequencyTracker>) -> Vec<AppResult> {
    // 如果需要，在后台更新缓存
    app_cache.update_if_needed();
    
    // 使用缓存的应用列表
    let all_apps = app_cache.get_apps();
    
    // 如果没有查询字符串，返回所有应用
    if query.is_empty() {
        return convert_to_results(&all_apps, &app_tracker);
    }
    
    // 按名称进行模糊匹配
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    let mut matched_apps: Vec<(i64, &AppInfo)> = Vec::new();
    
    for app in &all_apps {
        if let Some(score) = matcher.fuzzy_match(&app.name.to_lowercase(), &query.to_lowercase()) {
            matched_apps.push((score, app));
        }
    }
    
    // 按匹配分数排序
    matched_apps.sort_by(|a, b| b.0.cmp(&a.0));
    
    // 只返回匹配的应用，不包括分数
    convert_to_results(&matched_apps.into_iter().map(|(_, app)| app.clone()).collect::<Vec<_>>(), &app_tracker)
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
            
            // 提取图标
            let icon_path = extract_icon_from_exe(&path);
            
            AppResult {
                result_type: "app".to_string(),
                title: file_name,
                path,
                icon_path,
            }
        })
        .collect()
}

impl AppCache {
    fn new() -> Self {
        let cache = Self {
            apps: Arc::new(Mutex::new(Vec::new())),
            last_update: Arc::new(Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(600))),
            is_updating: Arc::new(AtomicBool::new(false)),
        };
        
        // 立即触发后台更新
        cache.update_if_needed();
        
        cache
    }
    
    fn get_apps(&self) -> Vec<AppInfo> {
        let apps = self.apps.lock().unwrap().clone();
        
        // 如果缓存为空且不在更新中，则进行同步初始化
        if apps.is_empty() && !self.is_updating.load(Ordering::SeqCst) {
            println!("Cache is Null!! Start Init...");
            let fresh_apps = search_windows_apps(""); // 使用原来的函数确保兼容性
            let mut cache_apps = self.apps.lock().unwrap();
            *cache_apps = fresh_apps;
            return cache_apps.clone();
        }
        
        apps
    }
    
    fn update_if_needed(&self) {
        let now = std::time::Instant::now();
        let update_needed = {
            let last = self.last_update.lock().unwrap();
            now.duration_since(*last).as_secs() > 300 // 5分钟更新一次
        };
        
        if update_needed && !self.is_updating.load(Ordering::SeqCst) {
            self.is_updating.store(true, Ordering::SeqCst);
            
            // 克隆Arc指针以便在新线程中安全使用
            let apps_clone = Arc::clone(&self.apps);
            let last_update_clone = Arc::clone(&self.last_update);
            let is_updating_clone = Arc::clone(&self.is_updating);
            
            std::thread::spawn(move || {
                println!("Updating Cache...");
                let start_time = std::time::Instant::now();
                
                let apps = collect_all_apps();
                
                {
                    let mut cache_apps = apps_clone.lock().unwrap();
                    *cache_apps = apps;
                    
                    let mut last_update = last_update_clone.lock().unwrap();
                    *last_update = std::time::Instant::now();
                }
                
                is_updating_clone.store(false, Ordering::SeqCst);
                println!("Updated Cache Success!! Consume time: {:?}", start_time.elapsed());
            });
        }
    }
}

// 将AppInfo转换为AppResult并考虑使用频率
fn convert_to_results(apps: &[AppInfo], app_tracker: &State<'_, AppFrequencyTracker>) -> Vec<AppResult> {
    let mut results: Vec<(i64, AppResult)> = Vec::new();
    
    for app in apps {
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
            title: app.name.clone(),
            path: app.path.clone(),
            icon_path: app.icon_path.clone(),
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

// 收集所有应用的函数
fn collect_all_apps() -> Vec<AppInfo> {
    use rayon::prelude::*;
    
    let mut all_apps = Vec::new();
    
    // 并行获取特殊文件夹中的快捷方式
    let shortcuts = get_shortcuts_from_special_folders();
    all_apps.extend(shortcuts);
    
    // 并行获取注册表中的应用
    let registry_apps = get_installed_apps_from_registry();
    all_apps.extend(registry_apps);
    
    // 并行提取图标
    all_apps.par_iter_mut().for_each(|app| {
        if app.icon_path.is_none() {
            app.icon_path = extract_icon_from_exe(&app.path);
        }
    });
    
    all_apps
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
        Err(e) => Err(format!("Open App Failed: {}", e))
    }
}

// 打开URL的命令
#[tauri::command]
async fn open_url(url: &str) -> Result<(), String> {
    match tauri_plugin_opener::open_url(url, Option::<&str>::None) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Open URL Failed: {}", e))
    }
}

// 执行网络搜索
#[tauri::command]
fn search_web(query: &str) -> Result<(), String> {
    let search_url = format!("https://www.google.com/search?q={}", query);
    
    match open::that(&search_url) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Failed to perform a web search: {}", e))
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

#[tauri::command]
async fn hide_main_window(window: tauri::Window) -> Result<(), String> {
    // 使用传递进来的 window 对象直接隐藏
    window.hide().map_err(|e| e.to_string())?;
    println!("Window hidden via backend command."); // 可选的日志
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 设置全局热键
            if let Err(e) = setup_global_hotkeys(app) {
                eprintln!("Failed to set global hotkey: {}", e);
            }

            // 创建并显示主窗口
            let main_window = app.get_webview_window("main").unwrap();
            main_window.set_title("BSearch").unwrap();

            Ok(())
        })
        // 确保 AppFrequencyTracker 被正确管理
        .manage(AppFrequencyTracker(Mutex::new(HashMap::new())))
        .manage(AppCache::new())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            search_apps,
            get_frequent_apps,
            get_icon_data,
            launch_app,
            open_url,
            search_web,
            hide_main_window
        ])
        .run(tauri::generate_context!())
        .expect("Runing Tauri App Error!!");
}