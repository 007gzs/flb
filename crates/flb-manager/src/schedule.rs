use chrono::{Local, Timelike};
use flb_cert::AcmeService;
use flb_store::Store;
use rand::Rng;
use std::sync::Arc;
use tracing::info;

pub fn spawn_renewal(store: Arc<Store>, acme: Arc<AcmeService>) {
    tokio::spawn(async move {
        let mut last_run: Option<chrono::NaiveDate> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let now = Local::now();
            if now.hour() != 0 {
                continue;
            }
            let today = now.date_naive();
            if last_run == Some(today) {
                continue;
            }
            let elapsed = now.minute() * 60 + now.second();
            let window = 3600u32.saturating_sub(elapsed);
            let delay = if window == 0 {
                0
            } else {
                rand::thread_rng().gen_range(0..window)
            };
            info!(delay_secs = delay, "scheduled ACME renewal window");
            tokio::time::sleep(std::time::Duration::from_secs(delay as u64)).await;
            last_run = Some(today);
            info!("running nightly ACME renewal");
            acme.renew_due(&store).await;
        }
    });
}
