use commands::calendar::fetch_calendar_events;

use std::sync::Mutex;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_sql::{Migration, MigrationKind};
use tauri::{Manager, State, LogicalSize};

mod commands;

struct AppState {
    receipt_visible: Mutex<bool>,
    table_visible: Mutex<bool>,
}

#[tauri::command]
fn resize_window_to_content(window: tauri::Window, height: u32) {
    let max_height: u32 = 950;
    let clamped = height.min(max_height);

    if let Ok(size) = window.inner_size() {
        // convert current physical size to logical for consistent sizing
        let scale = window.scale_factor().unwrap_or(1.0);
        let logical_width = size.width as f64 / scale;

        let _ = window.set_size(LogicalSize::new(logical_width, clamped as f64));
    }
}

fn toggle_window(app_handle: &tauri::AppHandle, label: &str, visible: &Mutex<bool>) {
    let mut v = visible.lock().unwrap();
    *v = !*v;

    if let Some(w) = app_handle.get_webview_window(label) {
        if *v {
            w.show().ok();
        } else {
            w.hide().ok();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let migrations = vec![
        Migration {
            version: 1,
            description: "create_initial_schema",
            sql: include_str!("../migrations/schema.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 2,
            description: "fix timely view local time",
            sql: "DROP VIEW timely;
                CREATE VIEW timely AS
                    SELECT \"id\", \"category\", \"task\", \"date\", \"status\"
                    FROM \"listAll\"
                    WHERE \"date\" >= datetime('now', 'localtime')
                        AND \"date\" < datetime('now', 'localtime', '+7 days')
                    ORDER BY \"date\";",
            kind: MigrationKind::Up,
        },
        Migration {
            version: 3,
            description: "add position and highlighted to task",
            sql: "ALTER TABLE task ADD COLUMN position INTEGER;
                ALTER TABLE task ADD COLUMN highlighted INTEGER NOT NULL DEFAULT 0;
                UPDATE task SET position = id WHERE position IS NULL;",
            kind: MigrationKind::Up,
        }
    ];

    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState {
            receipt_visible: Mutex::new(false),
            table_visible: Mutex::new(false),
        })
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:tasks.db", migrations)
                .build(),
        )
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();

            let h1 = handle.clone();
            app.global_shortcut()
                .on_shortcut("Control+S", move |app_handle, _s, e| {
                    if e.state() == ShortcutState::Pressed {
                        let state: State<AppState> = app_handle.state();
                        toggle_window(&h1, "receipt", &state.receipt_visible);
                        toggle_window(&h1, "table", &state.table_visible);
                    }
                })?;

            let h2 = handle.clone();
            app.global_shortcut()
                .on_shortcut("Control+H", move |app_handle, _s, e| {
                    if e.state() == ShortcutState::Pressed {
                        let state: State<AppState> = app_handle.state();
                        toggle_window(&h2, "receipt", &state.receipt_visible);
                    }
                })?;

            let h3 = handle.clone();
            app.global_shortcut()
                .on_shortcut("Control+T", move |app_handle, _s, e| {
                    if e.state() == ShortcutState::Pressed {
                        let state: State<AppState> = app_handle.state();
                        toggle_window(&h3, "table", &state.table_visible);
                    }
                })?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            fetch_calendar_events,
            resize_window_to_content
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}