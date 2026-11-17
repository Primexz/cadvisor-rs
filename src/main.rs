//! cadvisor-rs daemon entry point.
#![forbid(unsafe_code)]

use std::{
    error::Error,
    io::{self, Write},
    sync::Arc,
};

use cadvisor_rs::{
    collector::Collector,
    config::Config,
    docker::{DockerClient, DockerOptions, MetadataMap},
    metrics::{self, Labels},
    server::{self, Snapshot},
};
use clap::Parser;
use tokio::{
    sync::watch,
    time::{self, MissedTickBehavior},
};

type ErrorBox = Box<dyn Error + Send + Sync>;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ErrorBox> {
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cadvisor_rs=info".into()),
        )
        .init();
    let config = Config::parse();
    let labels = Labels::new(&config.docker_label)?;
    let mut collector = Collector::new(config.clone())?;
    let docker = if config.disable_docker {
        None
    } else {
        Some(DockerClient::with_options(
            &config.docker_socket,
            config.docker_label.clone(),
            DockerOptions {
                inspect: !config.disable_docker_inspect,
                running_only: config.docker_running_only,
                sizes: config.docker_size_metrics,
                max_containers: config.max_cgroups,
            },
        )?)
    };
    if config.once {
        let (metadata, engine) = match docker {
            Some(client) => {
                let (metadata, engine) = tokio::join!(client.refresh(), client.engine());
                (metadata, engine.ok())
            }
            None => (Ok(MetadataMap::new()), None),
        };
        let docker_up = !config.disable_docker && metadata.is_ok();
        let mut collection = collector.collect(&metadata.unwrap_or_default(), docker_up)?;
        collection.docker_engine = engine;
        io::stdout()
            .lock()
            .write_all(&metrics::encode(&collection, &labels))?;
        if collection.errors != 0 {
            return Err(io::Error::other("controller files could not be read").into());
        }
        return Ok(());
    }
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let (sender, receiver) = watch::channel(Arc::new(Snapshot::default()));
    let (stop, stopped) = watch::channel(false);
    let sampler = tokio::spawn(sample(
        collector,
        docker,
        labels,
        config.interval(),
        sender,
        stopped,
    ));
    tracing::info!(address = %listener.local_addr()?, "serving container metrics");
    let result = axum::serve(listener, server::router(receiver, config.stale_after()))
        .with_graceful_shutdown(shutdown())
        .await;
    let _ = stop.send(true);
    sampler.await??;
    result?;
    Ok(())
}

async fn sample(
    mut collector: Collector,
    docker: Option<DockerClient>,
    mut labels: Labels,
    interval: std::time::Duration,
    sender: watch::Sender<Arc<Snapshot>>,
    mut stopped: watch::Receiver<bool>,
) -> Result<(), ErrorBox> {
    let mut tick = time::interval(interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut metadata = MetadataMap::new();
    let mut docker_up = false;
    let mut engine = None;
    loop {
        tokio::select! {
            _ = stopped.changed() => break,
            _ = tick.tick() => {}
        }
        if collector.discovery_due()
            && let Some(client) = &docker
        {
            let (fresh, engine_result) = tokio::join!(client.refresh(), client.engine());
            engine = engine_result.ok();
            match fresh {
                Ok(fresh) => {
                    metadata = fresh;
                    docker_up = true;
                }
                Err(error) => {
                    docker_up = false;
                    tracing::warn!(%error, "Docker metadata unavailable");
                }
            }
        }
        let (returned, result) = tokio::task::spawn_blocking(move || {
            let result = collector
                .collect(&metadata, docker_up)
                .and_then(|mut collection| {
                    collection.docker_engine = engine.clone();
                    Snapshot::new(&collection, &labels).map_err(io::Error::other)
                });
            ((collector, metadata, labels, engine), result)
        })
        .await?;
        (collector, metadata, labels, engine) = returned;
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tracing::warn!(%error, "collection failed");
                Snapshot::default()
            }
        };
        sender.send_replace(Arc::new(snapshot));
    }
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            }
            Err(error) => {
                tracing::error!(%error, "cannot install SIGTERM handler");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
