// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::env;
use std::net::{IpAddr, ToSocketAddrs};
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
            // 每次跳转都重新校验目标地址，防止先通过校验的公网 URL 302 到内网/本机地址（SSRF）
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.stop();
                }
                match validate_public_url(attempt.url().as_str()) {
                    Ok(_) => attempt.follow(),
                    Err(_) => attempt.stop(),
                }
            }))
            .build()
            .expect("failed to create HTTP client")
    })
}

const MAX_BODY_BYTES: usize = 512 * 1024; // 512KB

// 判断一个 IP 是否是"公网地址"：私有网段、回环、链路本地(含 169.254.169.254 云元数据端点)、
// 组播、未指定地址等一律拒绝，防止后端被用来扫描/访问内网资源（SSRF）
fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast())
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            let is_unique_local = (segments[0] & 0xfe00) == 0xfc00; // fc00::/7
            let is_link_local = (segments[0] & 0xffc0) == 0xfe80; // fe80::/10
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || is_unique_local
                || is_link_local)
        }
    }
}

// 校验并规范化用户传入的 URL：必须是 http/https，且解析出的所有地址都必须是公网地址
fn validate_public_url(raw: &str) -> Result<String, String> {
    let target_url = if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else {
        format!("https://{}", raw)
    };

    let parsed = reqwest::Url::parse(&target_url).map_err(|e| format!("无效的 URL: {}", e))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("仅支持 http/https 协议".to_string());
    }
    let host = parsed.host_str().ok_or("URL 缺少主机名")?;
    let port = parsed.port_or_known_default().unwrap_or(443);

    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("域名解析失败: {}", e))?;

    let mut resolved_any = false;
    for addr in addrs {
        resolved_any = true;
        if !is_public_ip(addr.ip()) {
            return Err("出于安全考虑，禁止访问内网/本机地址".to_string());
        }
    }
    if !resolved_any {
        return Err("域名解析失败".to_string());
    }

    Ok(target_url)
}

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
// 便携版：数据文件默认放在可执行文件同级目录，整个文件夹搬去哪台电脑都带着数据走
// （注意：CARGO_MANIFEST_DIR 只在 cargo/`tauri dev` 启动时才存在，打包后的 exe 里没有这个环境变量）
fn portable_bookmarks_path() -> Result<PathBuf, String> {
    let exe_path = env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = exe_path.parent().ok_or("无法找到可执行文件所在目录")?;
    Ok(exe_dir.join("bookmarks.json"))
}

// 部分安装方式（如 MSI 默认装到 Program Files）下，可执行文件目录对普通用户只读，
// 便携路径写不进去时改存到系统的应用数据目录（Windows: %APPDATA%\<identifier>）
fn fallback_bookmarks_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("bookmarks.json"))
}

// 读取时优先用已经存在数据的那个位置，避免因为回退逻辑导致"数据换地方了找不到"
fn resolve_existing_bookmarks_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let portable = portable_bookmarks_path()?;
    if portable.exists() {
        return Ok(portable);
    }
    let fallback = fallback_bookmarks_path(app)?;
    if fallback.exists() {
        return Ok(fallback);
    }
    // 两边都还没有数据（全新安装）：默认仍然优先尝试便携路径
    Ok(portable)
}

// 2. 读取书签 API
#[tauri::command]
fn load_bookmarks(app: tauri::AppHandle) -> Result<String, String> {
    let path = resolve_existing_bookmarks_path(&app)?;
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
fn save_bookmarks(app: tauri::AppHandle, content: String) -> Result<(), String> {
    let path = resolve_existing_bookmarks_path(&app)?;
    match fs::write(&path, &content) {
        Ok(()) => Ok(()),
        // 便携路径不可写（多为安装到只读目录）：自动改存到应用数据目录再重试一次
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let fallback = fallback_bookmarks_path(&app)?;
            fs::write(&fallback, &content).map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

// 4. 解析 URL 自动获取描述 API
#[tauri::command]
async fn fetch_website_description(url: String) -> Result<String, String> {
    let target_url = validate_public_url(&url)?;

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
    let target_url = validate_public_url(&url)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_loopback_and_private_ranges() {
        assert!(validate_public_url("http://127.0.0.1").is_err());
        assert!(validate_public_url("http://localhost").is_err());
        assert!(validate_public_url("http://192.168.1.1").is_err());
        assert!(validate_public_url("http://10.0.0.5").is_err());
        assert!(validate_public_url("http://172.16.0.1").is_err());
    }

    #[test]
    fn rejects_link_local_and_cloud_metadata_endpoint() {
        // 169.254.169.254 是 AWS/GCP/Azure 云环境里获取实例元数据（含临时密钥）的经典 SSRF 目标
        assert!(validate_public_url("http://169.254.169.254").is_err());
        assert!(validate_public_url("http://169.254.1.1").is_err());
    }

    #[test]
    fn rejects_non_http_scheme() {
        assert!(validate_public_url("file:///etc/passwd").is_err());
        assert!(validate_public_url("ftp://example.com").is_err());
    }

    #[test]
    fn accepts_public_ip_literal() {
        assert!(validate_public_url("http://8.8.8.8").is_ok());
    }

    #[test]
    fn is_public_ip_rejects_reserved_ranges() {
        assert!(!is_public_ip("127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("10.1.2.3".parse().unwrap()));
        assert!(!is_public_ip("192.168.0.1".parse().unwrap()));
        assert!(!is_public_ip("169.254.169.254".parse().unwrap()));
        assert!(!is_public_ip("::1".parse().unwrap()));
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
        assert!(is_public_ip("1.1.1.1".parse().unwrap()));
    }
}