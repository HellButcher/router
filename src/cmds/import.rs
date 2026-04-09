use crate::args::import::ImportArgs;
use crate::config::RouterConfig;

pub async fn import(args: &ImportArgs, config: RouterConfig) -> anyhow::Result<()> {
    let source = args
        .source
        .clone()
        .or(config.import.source)
        .ok_or_else(|| anyhow::anyhow!(
            "no source file specified — use a positional argument or set `import.source` in the config"
        ))?;

    let storage_dir = args.storage_dir.clone().unwrap_or(config.storage.dir);

    let mut importer = router_import_pbf::Importer::from_path(&source)?
        .with_target_dir(storage_dir)
        .with_maxspeed(config.maxspeed);

    if let Some(path) = config.import.country_boundaries {
        importer = importer.with_country_boundaries(path);
    }

    tokio::task::spawn_blocking(move || importer.import()).await??;
    Ok(())
}
