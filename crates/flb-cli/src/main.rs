use clap::Parser;
use flb_cert::AcmeService;
use flb_config::Settings;
use flb_log::{FileLogger, init_tracing};
use flb_proxy::ProxyState;
use flb_store::Store;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "flb", about = "Fast Load Balancing")]
struct Cli {
    /// 配置管理服务监听地址
    #[arg(long, env = "FLB_ADMIN_LISTEN", default_value = "0.0.0.0:9000")]
    admin_listen: SocketAddr,
    /// HTTP 代理监听地址
    #[arg(long, env = "FLB_HTTP_LISTEN", default_value = "0.0.0.0:80")]
    http_listen: String,
    /// HTTPS 代理监听地址
    #[arg(long, env = "FLB_HTTPS_LISTEN", default_value = "0.0.0.0:443")]
    https_listen: String,
    /// 配置与证书数据目录
    #[arg(long, env = "FLB_DATA_DIR", default_value = "data")]
    data_dir: PathBuf,
    /// 前端静态资源目录（存在 index.html 时覆盖内嵌 UI）
    #[arg(long, env = "FLB_WWW_DIR", default_value = "www")]
    www_dir: PathBuf,
    /// 使用 Let's Encrypt 预发环境
    #[arg(long, env = "FLB_ACME_STAGING", default_value_t = false)]
    acme_staging: bool,
    /// 管理界面用户名
    #[arg(long, env = "FLB_ADMIN_USER", default_value = "admin")]
    admin_user: String,
    /// 管理界面密码
    #[arg(long, env = "FLB_ADMIN_PASSWORD", default_value = "admin")]
    admin_password: String,
    /// JWT 签名密钥；为空时由用户名和密码派生
    #[arg(long, env = "FLB_JWT_SECRET", default_value = "")]
    jwt_secret: String,
    /// HTTP 代理工作线程数，0 表示使用 CPU 核数
    #[arg(long, env = "FLB_THREADS", default_value_t = 0)]
    threads: usize,
    /// 同时处理的 HTTP 请求上限，超出返回 503
    #[arg(long, env = "FLB_MAX_INFLIGHT", default_value_t = 65536)]
    max_inflight: usize,
    /// 是否写入 access.log；压测对比时可关闭
    #[arg(long, env = "FLB_ACCESS_LOG", default_value_t = true)]
    access_log: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    std::fs::create_dir_all(&cli.data_dir)?;
    let settings = Settings {
        admin_listen: cli.admin_listen,
        http_listen: cli.http_listen,
        https_listen: cli.https_listen,
        data_dir: cli.data_dir,
        www_dir: cli.www_dir,
        acme_staging: cli.acme_staging,
        admin_user: cli.admin_user.clone(),
        admin_password: cli.admin_password.clone(),
        jwt_secret: if cli.jwt_secret.is_empty() {
            format!("flb-jwt\n{}\n{}", cli.admin_user, cli.admin_password)
        } else {
            cli.jwt_secret
        },
        threads: cli.threads,
        access_log: cli.access_log,
        max_inflight: cli.max_inflight.max(1),
    };
    let error_log = FileLogger::open(settings.error_log_path())?;
    let access_log = FileLogger::open(settings.access_log_path())?;
    let audit_log = FileLogger::open(settings.audit_log_path())?;
    init_tracing(error_log);
    std::fs::create_dir_all(settings.acme_dir())?;
    let legacy_account = settings.data_dir.join("acme-account.json");
    if legacy_account.exists() && !settings.acme_account_path().exists() {
        std::fs::rename(&legacy_account, settings.acme_account_path())?;
    }

    let store = Arc::new(Store::open(settings.config_path())?);
    let acme = Arc::new(AcmeService::new(
        settings.acme_account_path(),
        settings.acme_staging,
    ));

    let admin_settings = settings.clone();
    let admin_store = store.clone();
    let admin_acme = acme.clone();
    std::thread::Builder::new()
        .name("flb-admin".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("admin runtime");
            if let Err(err) = rt.block_on(flb_manager::run(
                admin_settings,
                admin_store,
                admin_acme,
                audit_log,
            )) {
                tracing::error!(error = %err, "admin server exited");
            }
        })?;

    let stream_store = store.clone();
    std::thread::Builder::new()
        .name("flb-stream".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("stream runtime");
            rt.block_on(flb_proxy::run_streams(stream_store));
        })?;

    info!(
        http = %settings.http_listen,
        https = %settings.https_listen,
        admin = %settings.admin_listen,
        "starting flb proxy"
    );
    let state = ProxyState::new(store, acme.http01.clone(), Arc::new(settings), access_log);
    flb_proxy::run_http_proxy(state).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}
