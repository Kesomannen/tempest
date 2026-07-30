use anyhow::Context;
use clap::Parser;
use loadsmith::{
    GithubRegistry, LocalRegistry, RegistrySet, ThunderstoreRegistry, thunderstore::SqliteIndex,
};
use tempest::Source;
use tracing::{debug, error, trace, warn};
use tracing_indicatif::IndicatifLayer;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

const DEFAULT_DIRECTIVE: &str = "info";
const VERBOSE_DIRECTIVE: &str =
    "debug,h2=info,hyper_util=info,reqwest=info,globset=info,rustls=info,tower=info";

#[tokio::main]
async fn main() {
    if let Err(err) = try_main().await {
        error!("{err:?}");
        std::process::exit(1);
    }
}

async fn try_main() -> anyhow::Result<()> {
    let cli = tempest::Cli::parse();

    const LOG_ENV_VAR: &str = "TEMPEST_LOG";

    let rust_log_set = std::env::var(LOG_ENV_VAR).is_ok();

    let filter = if rust_log_set {
        EnvFilter::from_env(LOG_ENV_VAR)
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
        warn!("{LOG_ENV_VAR} is set, --verbose flag will be ignored");
    }

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install rustls default provider");

    let ctx = create_context(&cli)?;

    trace!(?ctx, "created context");

    cli.run(&ctx).await
}

fn create_context(cli: &tempest::Cli) -> anyhow::Result<tempest::Context> {
    let home_dir = dirs_next::home_dir()
        .context("failed to determine home directory")?
        .join(".tempest");

    std::fs::create_dir_all(&home_dir).context("failed to create home directory")?;

    let http = reqwest::Client::builder()
        .user_agent(format!("tempest/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to create HTTP client")?;

    let thunderstore_client = thunderstore::Client::builder()
        .with_client(http.clone())
        .build()
        .context("failed to initialise thunderstore client")?;
    let thunderstore_index = SqliteIndex::open(
        thunderstore_client.clone(),
        home_dir.join("thunderstore.db"),
    )
    .context("failed to open index")?;

    // let hexium_client = thunderstore::Client::builder()
    //     .with_client(http.clone())
    //     .with_base_url("https://valheim.hexium.gg")
    //     .build()
    //     .context("failed to initialise hexium client")?;
    // let hexium_index = SqliteIndex::open(hexium_client.clone(), home_dir.join("hexium.db"))
    //     .context("failed to open hexium index")?;

    let mut registry_set = RegistrySet::new();
    registry_set.add(Source::Local, LocalRegistry::new());
    registry_set.add(Source::Github, GithubRegistry::new());
    registry_set.add(
        Source::Thunderstore,
        ThunderstoreRegistry::sqlite(thunderstore_client.clone(), thunderstore_index.clone()),
    );
    // registry_set.add(
    //     Source::Hexium,
    //     ThunderstoreRegistry::sqlite(hexium_client.clone(), hexium_index.clone()),
    // );

    let working_dir = std::env::current_dir().context("failed to determine working directory")?;

    let config = tempest::Config::read(&home_dir)?.unwrap_or_else(|| {
        debug!("config file does not exist, using default config");
        tempest::Config::default()
    });

    let store = loadsmith::PackageStore::open(home_dir.join("store"))
        .context("failed to open mod store")?;

    let indexes = tempest::Indexes::new(vec![
        tempest::Index::new(thunderstore_index, Source::Thunderstore),
        // tempest::Index::new(hexium_index, Source::Hexium),
    ]);

    Ok(tempest::Context::new(
        http,
        working_dir,
        home_dir,
        cli.locked,
        config,
        store,
        registry_set,
        thunderstore_client,
        // hexium_client,
        indexes,
    ))
}
