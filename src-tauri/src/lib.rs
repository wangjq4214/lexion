mod wordbooks;

use std::fs;

use tauri::Manager;
use wordbooks::WordbookRepository;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_directory = app.path().app_data_dir()?;
            fs::create_dir_all(&data_directory)?;
            let repository = WordbookRepository::open(data_directory.join("wordbooks.sqlite"))?;
            app.manage(repository);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            wordbooks::list_wordbooks,
            wordbooks::import_wordbook,
            wordbooks::sample_wordbook
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
