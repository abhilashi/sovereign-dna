mod analysis;
mod commands;
mod db;
mod error;
mod parser;
mod reference;
mod report;
mod research;

use tauri::Manager;

pub use db::Database;
pub use error::AppError;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Initialize logging
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Initialize database in app data directory
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| {
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to get app data directory: {}", e),
                    ))
                })?;

            let database = db::initialize_database(&app_data_dir)
                .map_err(|e| {
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to initialize database: {}", e),
                    ))
                })?;

            app.manage(database);

            log::info!("Genome Studio initialized successfully");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Import
            commands::import::import_genome,
            // Genome management
            commands::genome::list_genomes,
            commands::genome::get_genome_summary,
            commands::genome::delete_genome,
            // Health risks
            commands::health::get_health_risks,
            commands::health::get_health_risk_detail,
            // Pharmacogenomics
            commands::pharma::get_pharmacogenomics,
            // Traits
            commands::traits::get_trait_predictions,
            // Ancestry
            commands::ancestry::get_ancestry_analysis,
            // Carrier status
            commands::carrier::get_carrier_status,
            // Research
            commands::research::fetch_research,
            commands::research::get_cached_research,
            // SNP Explorer
            commands::explorer::get_snps,
            commands::explorer::get_snp_detail,
            commands::explorer::export_snps,
            // Report
            commands::report::generate_report,
            // Genome Map
            commands::genomemap::get_genome_layout,
            commands::genomemap::get_region_snps,
            commands::genomemap::get_chromosome_density,
            commands::genomemap::get_analysis_overlay,
            // Reference databases
            commands::reference::download_reference_database,
            commands::reference::get_reference_databases_status,
            commands::reference::delete_reference_database,
            // Research digest
            commands::digest::scan_research,
            commands::digest::get_research_digest,
            commands::digest::get_new_research_count,
            // Ask genome
            commands::ask::ask_genome,
            // Local LLM
            commands::local_llm::check_local_llm,
            commands::local_llm::chat_local_llm,
            // Workbench
            commands::workbench::research_query,
            commands::workbench::chat_with_claude,
            commands::workbench::get_workbench_sessions,
            commands::workbench::save_workbench_chat,
            commands::workbench::get_workbench_chat,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
