use crate::args::import::*;

pub async fn import(args: &ImportArgs) -> anyhow::Result<()> {
    let mut config = match &args.config {
        Some(path) => router_import_pbf::config::ImportConfig::from_file(path)?,
        None => router_import_pbf::config::ImportConfig::default(),
    };
    if config.import.country_boundaries.is_none() {
        let default = std::path::PathBuf::from("data/country_boundaries.geojson");
        if default.exists() {
            config.import.country_boundaries = Some(default);
        }
    }
    let importer = router_import_pbf::Importer::from_path(&args.source)?.with_config(config);
    tokio::task::spawn_blocking(move || importer.import()).await??;
    Ok(())
}
