use tokio::time::{sleep, Duration};

#[allow(dead_code)]
pub async fn with_backoff<F, Fut, T, E>(mut op: F, retries: usize) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut attempt = 0usize;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(err) => {
                attempt += 1;
                if attempt > retries {
                    return Err(err);
                }
                let wait = (attempt as u64).saturating_mul(200);
                sleep(Duration::from_millis(wait)).await;
            }
        }
    }
}
