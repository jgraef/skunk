use tokio_util::sync::CancellationToken;

/// Resolves when the application receives SIGTERM on unix systems, or never on
/// other systems.
async fn sigterm() {
    #[cfg(unix)]
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .unwrap()
        .recv()
        .await;

    #[cfg(not(unix))]
    std::future::pending::<()>().await;
}

/// Returns a [`CancellationToken`] that will be triggered when Ctrl-C is
/// pressed, or (on Unix) when SIGTERM is received.
pub fn cancel_on_ctrlc_or_sigterm() -> CancellationToken {
    let token = CancellationToken::new();

    tokio::spawn({
        let token = token.clone();
        async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    // ctrl-c
                    tracing::info!("Received Ctrl-C. Shutting down.");
                }
                _ = sigterm() => {
                    // sigterm (on unix)
                    tracing::info!("Received SIGTERM. Shutting down.");
                }
                _ = token.cancelled() => {
                    // cancel signal from somewhere else
                }
            }

            token.cancel();
        }
    });

    token
}
