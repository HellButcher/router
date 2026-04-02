use crate::args::import::*;

pub async fn import(args: &ImportArgs) -> anyhow::Result<()> {
    let importer = router_import_pbf::Importer::from_path(&args.source)?;
    tokio::task::spawn_blocking(move || importer.import()).await??;
    Ok(())
}
