use clap::Args;

#[derive(Clone, Debug, Args)]
pub struct ServeArgs {
    /// Defines the interface and port to listen for http connections
    #[clap(default_value = "127.0.0.1:8080")]
    pub listen: String,
}
