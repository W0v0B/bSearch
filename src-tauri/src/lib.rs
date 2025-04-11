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

static ICON_ID: AtomicU64 = AtomicU64::new(1);
static INIT: Once = Once::new();

struct AppFrequencyTracker(Mutex<HashMap<String, u32>>);

#[derive(Clone, Debug, serde::Serialize)]
pub struct AppInfo {
    name: String,
    path: String,
    icon_path: Option<String>,
    is_shortcut: bool,
}

struct AppCache {
    apps: Arc<Mutex<Vec<AppInfo>>>,
    last_update: Arc<Mutex<std::time::Instant>>,
    is_updating: Arc<AtomicBool>,
}

#[derive(Serialize, Deserialize, Clone)]
struct AppResult {
    #[serde(rename = "type")]
    result_type: String,
    title: String,
    path: String,
    icon_path: Option<String>,
}

fn get_special_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    
    if let Ok(userprofile) = env::var("USERPROFILE") {
        folders.push(PathBuf::from(userprofile).join("Desktop"));
    }
    
    if let Ok(public) = env::var("PUBLIC") {
        folders.push(PathBuf::from(public).join("Desktop"));
    }
    
    if let Ok(appdata) = env::var("APPDATA") {
        folders.push(PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    
    if let Ok(programdata) = env::var("ProgramData") {
        folders.push(PathBuf::from(programdata).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    
    folders
}

fn find_executables_in_dir(dir: &Path, app_name: &str, apps: &mut Vec<AppInfo>) {
    for entry in WalkDir::new(dir)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "exe")) {
        
        apps.push(AppInfo {
            name: app_name.to_string(),
            path: entry.path().to_string_lossy().to_string(),
            icon_path: None,
            is_shortcut: false,
        });
        
        break;
    }
}

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

fn resolve_shortcut(shortcut_path: &str) -> Option<String> {
    use windows::Win32::System::Com::{IPersistFile, STGM};
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    unsafe {
        let _ = CoInitialize(None);

        let shell_link: IShellLinkW = CoCreateInstance(
            &ShellLink,
            None,
            CLSCTX_INPROC_SERVER
        ).ok()?;

        let persist_file: IPersistFile = shell_link.cast().ok()?;
        
        let wide_path: Vec<u16> = shortcut_path.encode_utf16().chain(std::iter::once(0)).collect();
        
        persist_file.Load(PCWSTR(wide_path.as_ptr()), STGM::default()).ok()?;

        let mut buffer = [0u16; 260];
        let mut fd = windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW::default();
        
        shell_link.GetPath(
            &mut buffer,
            &mut fd,
            0
        ).ok()?;
        
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let os_string = OsString::from_wide(&buffer[0..len]);
        
        os_string.into_string().ok()
    }
}

fn get_uninstall_apps(hkey: HKEY, apps: &mut Vec<AppInfo>) {
    if let Ok(uninstall) = RegKey::predef(hkey)
        .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall") {
        
        for key_result in uninstall.enum_keys().flatten() {
            if let Ok(app_key) = uninstall.open_subkey(&key_result) {
                if let (Ok(name), Ok(location)) = (
                    app_key.get_value::<String, _>("DisplayName"),
                    app_key.get_value::<String, _>("InstallLocation")
                ) {
                    if !location.is_empty() {
                        let location_path = PathBuf::from(&location);
                        find_executables_in_dir(&location_path, &name, apps);
                    }
                }
            }
        }
    }
}

fn get_installed_apps_from_registry() -> Vec<AppInfo> {
    let mut apps = Vec::new();
    
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
    
    get_uninstall_apps(HKEY_LOCAL_MACHINE, &mut apps);
    
    get_uninstall_apps(HKEY_CURRENT_USER, &mut apps);
    
    apps
}

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

fn extract_icon_from_exe(exe_path: &str) -> Option<String> {
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
        let _ = CoInitialize(None);
        
        let wide_path: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
        
        let hicon = windows::Win32::UI::Shell::ExtractIconW(
            Some(windows::Win32::Foundation::HINSTANCE(null_mut())),
            PCWSTR(wide_path.as_ptr()),
            0
        );
        
        if !hicon.is_invalid() {
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
            
            let hbmp = windows::Win32::Graphics::Gdi::CreateCompatibleBitmap(hdc, 48, 48);
            if hbmp.is_invalid() {
                let _ = windows::Win32::Graphics::Gdi::DeleteDC(hdc_mem);
                let _ = windows::Win32::Graphics::Gdi::ReleaseDC(None, hdc);
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(hicon);
                return None;
            }
            
            let old_obj = windows::Win32::Graphics::Gdi::SelectObject(hdc_mem, hbmp.into());
            
            let _ = windows::Win32::Graphics::Gdi::SetBkColor(hdc_mem, COLORREF(0x00FFFFFF));
            let _ = windows::Win32::Graphics::Gdi::SetBkMode(hdc_mem, windows::Win32::Graphics::Gdi::TRANSPARENT);
            
            let _ = windows::Win32::UI::WindowsAndMessaging::DrawIconEx(
                hdc_mem,
                0, 0,
                hicon,
                48, 48,
                0,
                None,
                windows::Win32::UI::WindowsAndMessaging::DI_NORMAL
            );
            
            let result = save_bitmap_as_png(hbmp, &icon_path);
            
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
    
    let default_icon_path = cache_dir.join("default_icon.png");
    
    if !default_icon_path.exists() {
        create_default_icon(&default_icon_path);
    }
    
    if default_icon_path.exists() {
        Some(default_icon_path.to_string_lossy().to_string())
    } else {
        None
    }
}

fn save_bitmap_as_png(hbmp: windows::Win32::Graphics::Gdi::HBITMAP, path: &Path) -> bool {
    unsafe {
        let mut bmi: windows::Win32::Graphics::Gdi::BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = 48;
        bmi.bmiHeader.biHeight = -48;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = windows::Win32::Graphics::Gdi::BI_RGB.0;
        
        let data_size = 48 * 48 * 4;
        let mut buffer = vec![0u8; data_size];
        
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
        
        let file = match File::create(path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let writer = std::io::BufWriter::new(file);
        
        use image::{ImageBuffer, Rgba};
        
        let mut img = ImageBuffer::<Rgba<u8>, _>::new(48, 48);
        
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

fn create_default_icon(path: &Path) -> bool {
    use image::{ImageBuffer, Rgba};
    
    let mut img = ImageBuffer::<Rgba<u8>, _>::new(48, 48);
    
    for y in 0..48 {
        for x in 0..48 {
            let dx = x as i32 - 24;
            let dy = y as i32 - 24;
            let color = if dx * dx + dy * dy < 20 * 20 {
                Rgba([100, 149, 237, 255])
            } else {
                Rgba([0, 0, 0, 0])
            };
    
            img.put_pixel(x, y, color);
        }
    }
    
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

pub fn search_windows_apps(query: &str) -> Vec<AppInfo> {
    let mut all_apps = Vec::new();
    
    let mut shortcuts = get_shortcuts_from_special_folders();
    
    for app in &mut shortcuts {
        if app.icon_path.is_none() {
            app.icon_path = extract_icon_from_exe(&app.path);
        }
    }
    all_apps.extend(shortcuts);
    
    let mut registry_apps = get_installed_apps_from_registry();
    
    for app in &mut registry_apps {
        if app.icon_path.is_none() {
            app.icon_path = extract_icon_from_exe(&app.path);
        }
    }
    all_apps.extend(registry_apps);
    
    if query.is_empty() {
        return all_apps;
    }
    
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    let mut matched_apps: Vec<(i64, AppInfo)> = Vec::new();
    
    for app in all_apps {
        if let Some(score) = matcher.fuzzy_match(&app.name.to_lowercase(), &query.to_lowercase()) {
            matched_apps.push((score, app));
        }
    }
    
    matched_apps.sort_by(|a, b| b.0.cmp(&a.0));
    
    matched_apps.into_iter()
        .map(|(_, app)| app)
        .collect()
}

#[tauri::command]
fn search_apps(query: &str, app_cache: State<'_, AppCache>, app_tracker: State<'_, AppFrequencyTracker>) -> Vec<AppResult> {
    app_cache.update_if_needed();
    
    let all_apps = app_cache.get_apps();
    
    if query.is_empty() {
        return convert_to_results(&all_apps, &app_tracker);
    }
    
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    let mut matched_apps: Vec<(i64, &AppInfo)> = Vec::new();
    
    for app in &all_apps {
        if let Some(score) = matcher.fuzzy_match(&app.name.to_lowercase(), &query.to_lowercase()) {
            matched_apps.push((score, app));
        }
    }
    
    matched_apps.sort_by(|a, b| b.0.cmp(&a.0));
    
    convert_to_results(&matched_apps.into_iter().map(|(_, app)| app.clone()).collect::<Vec<_>>(), &app_tracker)
}

#[tauri::command]
fn get_frequent_apps(app_tracker: State<'_, AppFrequencyTracker>) -> Vec<AppResult> {
    let tracker = app_tracker.0.lock().unwrap();
    
    let mut apps: Vec<(String, u32)> = tracker.clone().into_iter().collect();
    apps.sort_by(|a, b| b.1.cmp(&a.1));
    
    apps.into_iter()
        .take(6)
        .map(|(path, _)| {
            let file_name = PathBuf::from(&path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown App")
                .to_string();
            
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
        
        cache.update_if_needed();
        
        cache
    }
    
    fn get_apps(&self) -> Vec<AppInfo> {
        let apps = self.apps.lock().unwrap().clone();
        
        if apps.is_empty() && !self.is_updating.load(Ordering::SeqCst) {
            let fresh_apps = search_windows_apps("");
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
            now.duration_since(*last).as_secs() > 300
        };
        
        if update_needed && !self.is_updating.load(Ordering::SeqCst) {
            self.is_updating.store(true, Ordering::SeqCst);
            
            let apps_clone = Arc::clone(&self.apps);
            let last_update_clone = Arc::clone(&self.last_update);
            let is_updating_clone = Arc::clone(&self.is_updating);
            
            std::thread::spawn(move || {                
                let apps = collect_all_apps();
                
                {
                    let mut cache_apps = apps_clone.lock().unwrap();
                    *cache_apps = apps;
                    
                    let mut last_update = last_update_clone.lock().unwrap();
                    *last_update = std::time::Instant::now();
                }
                
                is_updating_clone.store(false, Ordering::SeqCst);
            });
        }
    }
}

fn convert_to_results(apps: &[AppInfo], app_tracker: &State<'_, AppFrequencyTracker>) -> Vec<AppResult> {
    let mut results: Vec<(i64, AppResult)> = Vec::new();
    
    for app in apps {
        let frequency = {
            let tracker_guard = app_tracker.0.lock().unwrap();
            *tracker_guard.get(&app.path).unwrap_or(&0)
        };
        
        const FREQUENCY_WEIGHT: i64 = 10;
        let score = if app.is_shortcut { 100 } else { 50 };
        let combined_score = score + (frequency as i64 * FREQUENCY_WEIGHT);
        
        results.push((combined_score, AppResult {
            result_type: "app".to_string(),
            title: app.name.clone(),
            path: app.path.clone(),
            icon_path: app.icon_path.clone(),
        }));
    }
    
    results.sort_by(|a, b| b.0.cmp(&a.0));
    
    results.into_iter()
        .map(|(_, app)| app)
        .take(10)
        .collect()
}

fn collect_all_apps() -> Vec<AppInfo> {
    use rayon::prelude::*;
    
    let mut all_apps = Vec::new();
    
    let shortcuts = get_shortcuts_from_special_folders();
    all_apps.extend(shortcuts);
    
    let registry_apps = get_installed_apps_from_registry();
    all_apps.extend(registry_apps);
    
    all_apps.par_iter_mut().for_each(|app| {
        if app.icon_path.is_none() {
            app.icon_path = extract_icon_from_exe(&app.path);
        }
    });
    
    all_apps
}

#[tauri::command]
fn launch_app(app_path: &str, app_tracker: State<'_, AppFrequencyTracker>) -> Result<(), String> {
    {
        let mut tracker = app_tracker.0.lock().unwrap();
        *tracker.entry(app_path.to_string()).or_insert(0) += 1;
    }
    
    match Command::new(app_path).spawn() {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Open App Failed: {}", e))
    }
}

#[tauri::command]
async fn open_url(url: &str) -> Result<(), String> {
    match tauri_plugin_opener::open_url(url, Option::<&str>::None) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Open URL Failed: {}", e))
    }
}

#[tauri::command]
fn search_web(query: &str) -> Result<(), String> {
    let search_url = format!("https://www.google.com/search?q={}", query);
    
    match open::that(&search_url) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Failed to perform a web search: {}", e))
    }
}

fn setup_global_hotkeys<R: Runtime>(app: &tauri::App<R>) -> Result<(), Box<dyn std::error::Error>> {
    let app_handle: AppHandle<R> = app.app_handle().clone();

    let hotkey_manager = GlobalHotKeyManager::new()?;
    let hotkey = HotKey::new(Some(Modifiers::SHIFT), Code::Space);
    hotkey_manager.register(hotkey)?;

    let receiver = GlobalHotKeyEvent::receiver();

    std::thread::spawn(move || {
        for _event in receiver.iter() {

            if let Some(window) = app_handle.get_webview_window("main") {
                match window.is_visible() {
                    Ok(visible) => {
                        if visible {
                            let _ = window.hide();
                            if let Err(e) = window.emit("window-hidden", ()) {
                                eprintln!("Failed to emit window-hidden event: {}", e);
                            }
                        } else {
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
    });

    app.manage(hotkey_manager);

    Ok(())
}

#[tauri::command]
async fn hide_main_window(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if let Err(e) = setup_global_hotkeys(app) {
                eprintln!("Failed to set global hotkey: {}", e);
            }

            let main_window = app.get_webview_window("main").unwrap();
            main_window.set_title("BSearch").unwrap();

            Ok(())
        })
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