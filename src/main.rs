use anyhow::Context;
use clap::Parser;
use loadsmith::{
    GithubRegistry, LocalRegistry, RegistrySet, ThunderstoreRegistry, thunderstore::SqliteIndex,
};
use tracing::{debug, error, warn};
use tracing_indicatif::IndicatifLayer;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

const DEFAULT_DIRECTIVE: &str = "info";
const VERBOSE_DIRECTIVE: &str = "trace,h2=info,hyper_util=info,reqwest=info,globset=info";

#[tokio::main]
async fn main() {
    if let Err(err) = try_main().await {
        error!("{err:?}");
        std::process::exit(1);
    }
}

async fn try_main() -> anyhow::Result<()> {
    let cli = tempest::Cli::parse();

    let rust_log_set = std::env::var("RUST_LOG").is_ok();

    let filter = if rust_log_set {
        EnvFilter::from_default_env()
    } else if cli.verbose {
        EnvFilter::new(VERBOSE_DIRECTIVE)
    } else {
        EnvFilter::new(DEFAULT_DIRECTIVE)
    };

    let indicatif_layer = IndicatifLayer::new();

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_ansi_sanitization(false)
                .without_time()
                .with_writer(indicatif_layer.get_stderr_writer()),
        )
        .with(indicatif_layer)
        .try_init()
        .context("failed to initialise logging")?;

    if cli.verbose && rust_log_set {
        warn!("RUST_LOG is set, verbose flag will be ignored");
    }

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install rustls default provider");

    let ctx = create_context(&cli)?;
    cli.run(&ctx).await
}

fn create_context(cli: &tempest::Cli) -> anyhow::Result<tempest::Context> {
    let home_dir = dirs_next::home_dir()
        .context("failed to determine home directory")?
        .join(".tempest");

    std::fs::create_dir_all(&home_dir).context("failed to create home directory")?;

    let http = reqwest::Client::new();
    let thunderstore = thunderstore::Client::builder()
        .with_client(http.clone())
        .build()
        .context("failed to initialise thunderstore client")?;
    let index = SqliteIndex::open(thunderstore.clone(), home_dir.join("index.db"))
        .context("failed to open index")?;

    let mut registry_set = RegistrySet::new();
    registry_set.add("local", LocalRegistry::new());
    registry_set.add(
        "thunderstore",
        ThunderstoreRegistry::sqlite(thunderstore.clone(), index.clone()),
    );
    registry_set.add("github", GithubRegistry::default());

    let working_dir = std::env::current_dir().context("failed to determine working directory")?;

    let config = tempest::Config::read(&home_dir)?.unwrap_or_else(|| {
        debug!("config file does not exist, using default config");
        tempest::Config::default()
    });

    Ok(tempest::Context::new(
        http,
        thunderstore,
        registry_set,
        index,
        working_dir,
        home_dir,
        cli.locked,
        config,
    ))
}
