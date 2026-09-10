use crate::hooks_install;
use crate::i18n::tr;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let lang = {
        let st = app.state::<crate::AppState>();
        let c = st.cfg.lock().unwrap();
        c.lang.clone()
    };
    let menu = build_menu(app, &lang)?;
    // The application's own icon rather than the monochrome tray glyph: see trayicon::app_mark
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-color.png"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip(concat!("Codenotch v", env!("CARGO_PKG_VERSION")))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, ev| handle(app, ev.id().as_ref()))
        .build(app)?;
    Ok(())
}

/// Deliberately short. Everything that used to live here — language, autostart, hooks, the tray
/// icon layout, the notch — now has a proper home in the settings window, which can explain each
/// choice instead of hiding it behind a two-word menu label.
pub fn build_menu(app: &AppHandle, lang: &str) -> tauri::Result<Menu<Wry>> {
    let settings = MenuItemBuilder::with_id("settings", tr(lang, "settings")).build(app)?;
    let refresh = MenuItemBuilder::with_id("refresh", tr(lang, "refresh")).build(app)?;
    let quit = MenuItemBuilder::with_id("quit", tr(lang, "quit")).build(app)?;
    MenuBuilder::new(app)
        .item(&settings)
        .item(&refresh)
        .separator()
        .item(&quit)
        .build()
}

/// Rebuilds the tray menu, ALWAYS on the main thread.
///
/// A menu is a Windows UI object. Building one or swapping it in from another thread leaves the
/// tray holding a menu that never opens again — and since changing the language is what triggers a
/// rebuild, the user is then locked out of the only place they could change it back. The tray's own
/// click handlers already run on the main thread, but commands from the settings window do not, so
/// the hop is done here once rather than being remembered at every call site.
pub fn refresh_menu(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let lang = {
            let st = handle.state::<crate::AppState>();
            let c = st.cfg.lock().unwrap();
            c.lang.clone()
        };
        if let Some(tray) = handle.tray_by_id("main") {
            if let Ok(menu) = build_menu(&handle, &lang) {
                let _ = tray.set_menu(Some(menu));
            }
        }
    });
}

fn handle(app: &AppHandle, id: &str) {
    match id {
        "install" => notice(app, hooks_install::install()),
        "uninstall" => notice(app, hooks_install::uninstall()),
        "reset" => crate::reset_bar(app),
        "open-data" => {
            let dir = crate::config::config_path().parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let _ = std::fs::create_dir_all(crate::glyphs::user_dir());
            let mut cmd = std::process::Command::new("explorer");
            cmd.arg(dir.as_os_str());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            let _ = cmd.spawn();
        }
        "refresh" => {
            {
                let st = app.state::<crate::AppState>();
                let mut u = st.usage.lock().unwrap();
                u.backoff_until = 0;
            }
            crate::usage::request_refresh();
            crate::codex::request_refresh();
            crate::cursor::request_refresh();
            crate::antigravity::request_refresh();
            let a = app.clone();
            std::thread::spawn(move || crate::reload_glyphs(&a));
        }
        "autostart" => {
            let r = if crate::autostart::is_enabled() {
                crate::autostart::disable()
            } else {
                crate::autostart::enable()
            };
            notice(app, r);
            refresh_menu(app); // refresh the check marks
        }
        "tim-off" | "tim-numbers" | "tim-bars" => {
            {
                let st = app.state::<crate::AppState>();
                let mut c = st.cfg.lock().unwrap();
                c.tray_mode = id.trim_start_matches("tim-").to_string();
                crate::config::save(&c);
            }
            refresh_menu(app);
        }
        _ if id.starts_with("tip-") => {
            let pid = id.trim_start_matches("tip-").to_string();
            {
                let st = app.state::<crate::AppState>();
                let mut c = st.cfg.lock().unwrap();
                if let Some(i) = c.tray_providers.iter().position(|x| *x == pid) {
                    c.tray_providers.remove(i);
                } else {
                    // Kept in the notch's own order, so the icon reads the same way the pill does
                    c.tray_providers.push(pid);
                    c.tray_providers.sort_by_key(|x| {
                        crate::TRAY_PROVIDER_IDS.iter().position(|p| p == x).unwrap_or(usize::MAX)
                    });
                }
                crate::config::save(&c);
            }
            refresh_menu(app);
        }
        "settings" => {
            if let Some(w) = app.get_webview_window("settings") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }
        "quit" => app.exit(0),
        _ if id.starts_with("lang-") => crate::apply_lang(app, &id[5..]),
        _ => {}
    }
}

fn notice(app: &AppHandle, r: Result<String, String>) {
    let msg = match r {
        Ok(m) => m,
        Err(e) => format!("Error: {e}"),
    };
    let _ = app.emit("notice", &msg);
}
