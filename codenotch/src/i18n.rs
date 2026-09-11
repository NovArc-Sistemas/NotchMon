//! Rust-side (tray menu) strings. The page has its own dictionary; keys are kept identical on both sides.

pub fn resolve_auto() -> &'static str {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buf = [0u16; 85];
        let n = GetUserDefaultLocaleName(&mut buf);
        if n > 0 {
            let name = String::from_utf16_lossy(&buf[..(n as usize - 1)]).to_lowercase();
            if name.starts_with("zh") {
                return "zh";
            }
            if name.starts_with("ja") {
                return "ja";
            }
            if name.starts_with("ko") {
                return "ko";
            }
            if name.starts_with("ru") {
                return "ru";
            }
        }
    }
    "en"
}

pub fn tr(lang: &str, key: &str) -> &'static str {
    let l = if lang == "auto" { resolve_auto() } else { lang };
    match (l, key) {
        ("zh", "install") => "安装 Claude Code 钩子",
        ("zh", "uninstall") => "卸载钩子",
        ("zh", "language") => "语言",
        ("zh", "lang_auto") => "跟随系统",
        ("zh", "reset_pos") => "重置悬浮条位置",
        ("zh", "quit") => "退出",
        ("zh", "hooks_missing") => "钩子未安装：右键托盘图标 → 安装 Claude Code 钩子（桌面版无需，已自动兜底）",
        ("zh", "autostart") => "开机自启（静默待命）",
        ("ja", "autostart") => "Windows起動時に自動開始",
        ("ko", "autostart") => "Windows 시작 시 자동 실행",
        ("zh", "refresh") => "立即刷新用量",
        ("zh", "open_data") => "打开数据文件夹（日志 / 图标）",
        ("ja", "open_data") => "データフォルダを開く（ログ / アイコン）",
        ("ko", "open_data") => "데이터 폴더 열기 (로그 / 아이콘)",
        ("ru", "open_data") => "Открыть папку данных (журналы / значки)",
        (_, "open_data") => "Open data folder (logs / icons)",
        ("ja", "refresh") => "使用量を今すぐ更新",
        ("ko", "refresh") => "사용량 지금 새로고침",
        ("ja", "install") => "Claude Code フックを導入",
        ("ja", "uninstall") => "フックを削除",
        ("ja", "language") => "言語",
        ("ja", "lang_auto") => "システムに従う",
        ("ja", "reset_pos") => "バー位置をリセット",
        ("ja", "quit") => "終了",
        ("ja", "hooks_missing") => "フック未導入：トレイ右クリック → フックを導入（デスクトップ版は自動フォールバック済み）",
        ("ko", "install") => "Claude Code 후크 설치",
        ("ko", "uninstall") => "후크 제거",
        ("ko", "language") => "언어",
        ("ko", "lang_auto") => "시스템 따르기",
        ("ko", "reset_pos") => "바 위치 초기화",
        ("ko", "quit") => "종료",
        ("ko", "hooks_missing") => "후크 미설치: 트레이 우클릭 → 후크 설치 (데스크톱판은 자동 폴백)",
        ("ru", "install") => "Установить хуки Claude Code",
        ("ru", "uninstall") => "Удалить хуки",
        ("ru", "language") => "Язык",
        ("ru", "lang_auto") => "Как в системе",
        ("ru", "reset_pos") => "Сбросить положение панели",
        ("ru", "quit") => "Выйти",
        ("ru", "hooks_missing") => "Хуки не установлены: нажмите правой кнопкой по значку в трее → Установить хуки Claude Code (для настольной версии используется автоматический резервный режим)",
        ("ru", "autostart") => "Запускать с Windows (в фоне)",
        ("ru", "refresh") => "Обновить использование",
        (_, "install") => "Install Claude Code hooks",
        (_, "uninstall") => "Uninstall hooks",
        (_, "language") => "Language",
        (_, "lang_auto") => "Follow system",
        (_, "reset_pos") => "Reset bar position",
        (_, "quit") => "Quit",
        (_, "hooks_missing") => "Hooks not installed: tray right-click → Install Claude Code hooks (desktop app auto-fallback active)",
        (_, "autostart") => "Start with Windows (silent)",
        (_, "refresh") => "Refresh usage now",
        ("zh", "settings") => "设置…",
        ("ja", "settings") => "設定…",
        ("ko", "settings") => "설정…",
        ("ru", "settings") => "Настройки…",
        (_, "settings") => "Settings…",

        ("zh", "tray_icon") => "托盘图标",
        ("ja", "tray_icon") => "トレイアイコン",
        ("ko", "tray_icon") => "트레이 아이콘",
        ("ru", "tray_icon") => "Значок в трее",
        (_, "tray_icon") => "Tray icon",

        ("zh", "tray_off") => "默认图标",
        ("ja", "tray_off") => "既定のアイコン",
        ("ko", "tray_off") => "기본 아이콘",
        ("ru", "tray_off") => "Обычный значок",
        (_, "tray_off") => "Plain icon",

        ("zh", "tray_numbers") => "数字（最多两项）",
        ("ja", "tray_numbers") => "数字（最大2件）",
        ("ko", "tray_numbers") => "숫자 (최대 2개)",
        ("ru", "tray_numbers") => "Числа (до 2)",
        (_, "tray_numbers") => "Numbers (up to 2)",

        ("zh", "tray_bars") => "条形图（多项）",
        ("ja", "tray_bars") => "バー（複数可）",
        ("ko", "tray_bars") => "막대 (여러 개)",
        ("ru", "tray_bars") => "Полосы (больше 2)",
        (_, "tray_bars") => "Bars (more than 2)",

        ("zh", "tray_which") => "显示哪些",
        ("ja", "tray_which") => "対象",
        ("ko", "tray_which") => "표시 대상",
        ("ru", "tray_which") => "Какие провайдеры",
        (_, "tray_which") => "Which providers",

        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::tr;

    const RUSSIAN_KEYS: &[(&str, &str)] = &[
        ("settings", "Настройки…"),
        ("refresh", "Обновить использование"),
        ("quit", "Выйти"),
        ("install", "Установить хуки Claude Code"),
        ("uninstall", "Удалить хуки"),
        ("language", "Язык"),
        ("lang_auto", "Как в системе"),
        ("reset_pos", "Сбросить положение панели"),
        ("hooks_missing", "Хуки не установлены: нажмите правой кнопкой по значку в трее → Установить хуки Claude Code (для настольной версии используется автоматический резервный режим)"),
        ("autostart", "Запускать с Windows (в фоне)"),
        ("open_data", "Открыть папку данных (журналы / значки)"),
        ("tray_icon", "Значок в трее"),
        ("tray_off", "Обычный значок"),
        ("tray_numbers", "Числа (до 2)"),
        ("tray_bars", "Полосы (больше 2)"),
        ("tray_which", "Какие провайдеры"),
    ];

    #[test]
    fn russian_translates_every_known_key() {
        for (key, value) in RUSSIAN_KEYS {
            assert_eq!(
                tr("ru", key),
                *value,
                "missing Russian translation for {key}"
            );
            assert_ne!(tr("ru", key), "?", "unknown Russian key {key}");
        }
    }

    #[test]
    fn unknown_language_keeps_the_english_fallback() {
        assert_eq!(tr("xx", "settings"), "Settings…");
    }
}
