async fn watch() -> eyre::Result<()> {
    // let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    //
    // tokio::spawn(async move {
    //     tokio::signal::ctrl_c().await.unwrap();
    //     tracing::warn!("received ctr-c");
    //     tracing::info!("initiate graceful shutdown");
    //     shutdown_tx.send(true).unwrap();
    // });
    Ok(())
}
