use std::{collections::HashMap, path::PathBuf};

use router_service::speed_config::SpeedConfig;
use serde::Deserialize;

/// Default config file name searched in the current directory.
pub const DEFAULT_CONFIG_FILE: &str = "router-config.toml";

/// Top-level unified configuration.
///
/// Relevant sections per subcommand:
///
/// - `import`:          `[import]`, `[storage]`, `[maxspeed]`
/// - `serve` / `route`: `[storage]`, `[server]`, `[country_speed.*.*]`
///
/// Example `router-config.toml`:
///
/// ```toml
/// [import]
/// source = "germany-latest.osm.pbf"
/// country_boundaries = "data/country_boundaries.geojson"
///
/// [storage]
/// dir = "storage"
///
/// [server]
/// listen = "127.0.0.1:5173"
///
/// [maxspeed]
/// "DE:urban" = 50
///
/// [country_speed.DE.car]
/// motorway = 130
/// trunk    = 100
/// ```
#[derive(Debug, Default, Deserialize)]
pub struct RouterConfig {
    #[serde(default)]
    pub import: ImportSection,
    #[serde(default)]
    pub storage: StorageSection,
    #[serde(default)]
    pub server: ServerSection,
    /// Named maxspeed overrides for the importer (import-time only).
    #[serde(default)]
    pub maxspeed: HashMap<String, u8>,
    /// Per-country, per-profile speed overrides for the routing service (serve-time only).
    #[serde(flatten)]
    pub speeds: SpeedConfig,
}

#[derive(Debug, Default, Deserialize)]
pub struct ImportSection {
    /// Path to the OSM PBF source file.
    pub source: Option<PathBuf>,
    /// Path to a GeoJSON file with country boundary polygons.
    pub country_boundaries: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
pub struct StorageSection {
    /// Directory for imported data files.
    pub dir: PathBuf,
}

impl Default for StorageSection {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("storage"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ServerSection {
    /// Interface and port to listen on.
    pub listen: String,
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:5173".to_string(),
        }
    }
}

impl RouterConfig {
    /// Load from a TOML file. Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: &std::path::Path) -> Result<Self, ConfigError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| ConfigError::Io(path.to_path_buf(), e))?;
        toml::from_str(&content).map_err(|e| ConfigError::Parse(path.to_path_buf(), e))
    }

    /// Load configuration using the standard resolution order:
    ///
    /// 1. If `explicit` is `Some`, load that file (it **must** exist — error otherwise).
    /// 2. Else if `router-config.toml` exists in the current directory, load it.
    /// 3. Otherwise return `RouterConfig::default()`.
    ///
    /// Paths in the config file are resolved relative to the config file's directory.
    pub fn load(explicit: Option<&PathBuf>) -> Result<Self, ConfigError> {
        if let Some(path) = explicit {
            let mut config = Self::from_file(path)?;
            let base = path.parent().unwrap_or(std::path::Path::new("."));
            config.resolve_paths(base);
            return Ok(config);
        }
        let default_path = PathBuf::from(DEFAULT_CONFIG_FILE);
        if default_path.exists() {
            let mut config = Self::from_file(&default_path)?;
            let base = default_path.parent().unwrap_or(std::path::Path::new("."));
            config.resolve_paths(base);
            return Ok(config);
        }
        let mut config = Self::default();
        config.resolve_paths(std::path::Path::new("."));
        Ok(config)
    }

    /// Resolve all relative paths against `base` and apply the country-boundaries fallback.
    fn resolve_paths(&mut self, base: &std::path::Path) {
        let resolve = |p: &PathBuf| -> PathBuf {
            if p.is_relative() {
                base.join(p)
            } else {
                p.clone()
            }
        };
        self.import.source = self.import.source.as_ref().map(&resolve);
        self.import.country_boundaries = self.import.country_boundaries.as_ref().map(&resolve);
        if self.import.country_boundaries.is_none() {
            let fallback = base.join("data/country_boundaries.geojson");
            if fallback.exists() {
                self.import.country_boundaries = Some(fallback);
            }
        }
        self.storage.dir = resolve(&self.storage.dir);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file {0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("failed to parse config file {0}: {1}")]
    Parse(PathBuf, toml::de::Error),
}
