use clap::Parser;
use jxlify::cache::cleaner::start_cache_cleaner;
use jxlify::cache::CacheManager;
use jxlify::config::{CliArgs, JxlifyConfig, BANNER, DEFAULT_VERSION, SAMPLE_TOML_CONFIG};
use jxlify::encoder::prefetch::prefetch_images;
use jxlify::handler::{build_router, AppState};
use std::sync::Arc;
use tokio::signal;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    jxlify::init();
    let args = CliArgs::parse();

    // 1. Handle --dump-config
    if args.dump_config {
        println!("{}", SAMPLE_TOML_CONFIG);
        return Ok(());
    }

    // 2. Setup Logging
    let log_level = match args.verbosity {
        0 => Level::ERROR,
        1 => Level::ERROR,
        2 => Level::WARN,
        3 => Level::INFO,
        _ => Level::DEBUG,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber).ok();

    // 3. Banner & Version
    println!("{}", BANNER);
    info!("JXLify version v{}", DEFAULT_VERSION);

    // 4. Load Config
    let config = JxlifyConfig::load_from_file_or_default(&args.config);
    info!("Allowed source types: {:?}", config.allowed_types);
    info!("Conversion to JXL enabled: {}", config.enable_jxl);
    info!("Conversion to AVIF enabled: {}", config.enable_avif);
    info!("Conversion to WebP enabled: {}", config.enable_webp);
    info!("Image quality: {}", config.quality);
    info!("Image origin path: {}", config.img_path);
    info!("Unified cache path: {}", config.cache_path);

    // 5. Handle Standalone Foreground Prefetch
    if args.prefetch_foreground {
        info!("Running foreground prefetch and exiting...");
        prefetch_images(config.clone(), args.jobs).await;
        return Ok(());
    }

    // 6. Handle Background Prefetch
    if args.prefetch {
        let cfg_clone = config.clone();
        tokio::spawn(async move {
            prefetch_images(cfg_clone, args.jobs).await;
        });
    }

    // 7. Spawn Cache Cleaner Background Service
    let cleaner_config = config.clone();
    tokio::spawn(async move {
        start_cache_cleaner(cleaner_config).await;
    });

    // 8. Build Router and State
    let cache_manager = CacheManager::new();
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let state = AppState {
        config: Arc::new(config.clone()),
        cache: cache_manager,
        http_client,
    };

    let app = build_router(state);

    // 9. Bind TCP Listener
    let bind_addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!("JXLify server is listening on http://{}", bind_addr);

    // 10. Serve with Graceful Shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("JXLify server stopped gracefully.");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down...");
        },
        _ = terminate => {
            info!("Received SIGTERM, shutting down...");
        },
    }
}
