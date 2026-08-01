// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;
use reqwest;
use scraper::{Html, Selector};
use tauri::Manager;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn get_http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to create HTTP client")
    })
}

const MAX_BODY_BYTES: usize = 512 * 1024; // 512KB

// 显示/隐藏主窗口切换：托盘左键单击和全局热键共用同一套逻辑
fn toggle_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        if is_visible {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

// 1. 确定 bookmarks.json 存放的具体路径
// 便携版：数据文件放在可执行文件同级目录，整个文件夹搬去哪台电脑都带着数据走
// （注意：CARGO_MANIFEST_DIR 只在 cargo/`tauri dev` 启动时才存在，打包后的 exe 里没有这个环境变量）
fn get_bookmarks_path() -> Result<PathBuf, String> {
    let exe_path = env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = exe_path.parent().ok_or("无法找到可执行文件所在目录")?;
    Ok(exe_dir.join("bookmarks.json"))
}
// 2. 读取书签 API
#[tauri::command]
fn load_bookmarks() -> Result<String, String> {
    let path = get_bookmarks_path()?;
    if path.exists() {
        // 如果文件存在，直接读取内容
        fs::read_to_string(path).map_err(|e| e.to_string())
    } else {
        // 如果文件不存在，返回一个初始化的空 JSON 字符串
        Ok(r#"{"categories":[], "bookmarks":[]}"#.to_string())
    }
}

// 3. 保存书签 API
#[tauri::command]
fn save_bookmarks(content: String) -> Result<(), String> {
    let path = get_bookmarks_path()?;
    // 将前端传过来的 JSON 字符串写入文件
    fs::write(path, content).map_err(|e| e.to_string())
}

// 4. 解析 URL 自动获取描述 API
#[tauri::command]
async fn fetch_website_description(url: String) -> Result<String, String> {
    let target_url = if url.starts_with("http://") || url.starts_with("https://") {
        url
    } else {
        format!("https://{}", url)
    };

    let client = get_http_client();
    let response = client.get(&target_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let bytes = response.bytes()
        .await
        .map_err(|e| format!("读取网页内容失败: {}", e))?;
    let html_content = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_BODY_BYTES)]);

    let document = Html::parse_document(&html_content);
    let selector = Selector::parse("meta[name='description']").map_err(|_| "解析 HTML 选择器失败")?;

    if let Some(element) = document.select(&selector).next() {
        if let Some(content) = element.value().attr("content") {
            return Ok(content.trim().to_string());
        }
    }

    Ok("暂无描述信息...".to_string())
}

// 5. 一次请求同时抓取标题 + 描述，用于添加书签时自动填充
//    带完整浏览器请求头减少被拦截概率，多标签降级（title → og:title → twitter:title）
#[tauri::command]
async fn fetch_website_meta(url: String) -> Result<String, String> {
    let target_url = if url.starts_with("http://") || url.starts_with("https://") {
        url
    } else {
        format!("https://{}", url)
    };

    let client = get_http_client();
    let response = client.get(&target_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
        .header("Connection", "keep-alive")
        .header("Upgrade-Insecure-Requests", "1")
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let bytes = response.bytes()
        .await
        .map_err(|e| format!("读取网页内容失败: {}", e))?;
    let html_content = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_BODY_BYTES)]);

    let document = Html::parse_document(&html_content);

    let extract_attr = |selector_str: &str| -> Option<String> {
        Selector::parse(selector_str).ok()
            .and_then(|sel| document.select(&sel).next())
            .and_then(|el| el.value().attr("content").map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
    };

    let title = Selector::parse("title").ok()
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| extract_attr("meta[property='og:title']"))
        .or_else(|| extract_attr("meta[name='twitter:title']"))
        .unwrap_or_default();

    let description = extract_attr("meta[name='description']")
        .or_else(|| extract_attr("meta[property='og:description']"))
        .or_else(|| extract_attr("meta[name='twitter:description']"))
        .unwrap_or_default();

    // 提取 favicon：依次尝试 link[rel*="icon"]、link[rel="shortcut icon"]
    let favicon = ["link[rel='icon']", "link[rel='shortcut icon']", "link[rel='apple-touch-icon']"]
        .iter()
        .filter_map(|sel_str| {
            Selector::parse(sel_str).ok()
                .and_then(|sel| document.select(&sel).next())
                .and_then(|el| el.value().attr("href").map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
        })
        .next()
        .map(|href| {
            if href.starts_with("http://") || href.starts_with("https://") {
                href
            } else if href.starts_with("//") {
                format!("https:{}", href)
            } else {
                // 相对路径 → 拼接到目标域名
                if let Ok(base) = reqwest::Url::parse(&target_url) {
                    base.join(&href).map(|u| u.to_string()).unwrap_or(href)
                } else {
                    href
                }
            }
        })
        .unwrap_or_default();

    let result = serde_json::json!({ "title": title, "description": description, "favicon": favicon });
    Ok(result.to_string())
}

// 核心运行函数
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .invoke_handler(tauri::generate_handler![
            load_bookmarks,
            save_bookmarks,
            fetch_website_description,
            fetch_website_meta,
        ])
        .setup(|app| {
            // 开机自启动：只注册一次，后续启动跳过
            let autostart = app.autolaunch();
            if !autostart.is_enabled().unwrap_or(false) {
                let _ = autostart.enable();
                println!("[autostart] 已注册开机自启动");
            }

            // 启动时默认隐藏窗口 → 只挂托盘 + 热键，不创建 WebView 渲染，内存最低
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
            // 系统托盘：左键单击图标 = 显示/聚焦主窗口；菜单里提供“显示”和“退出”
            let show_item = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&show_item, &quit_item]).build()?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("书签导航")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        toggle_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // 全局快捷键 Alt+Space：唤出/隐藏主窗口
            let toggle_shortcut = Shortcut::new(Some(Modifiers::ALT), Code::Space);
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |app, shortcut, event| {
                        if shortcut == &toggle_shortcut && event.state() == ShortcutState::Pressed {
                            toggle_main_window(app);
                        }
                    })
                    .build(),
            )?;

            // 尝试注册；如果 Alt+Space 已经被其他程序占用，这里会失败——
            // 不让它中断启动，只在终端打印出来，方便排查热键冲突
            match app.global_shortcut().register(toggle_shortcut) {
                Ok(_) => println!("[hotkey] 全局快捷键 Alt+Space 注册成功"),
                Err(e) => eprintln!("[hotkey] 全局快捷键 Alt+Space 注册失败，可能已被其他程序占用: {}", e),
            }

            Ok(())
        })
        // 点关闭按钮时隐藏到托盘而不是退出进程，这样常驻后台不会有额外开销
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}