use std::{
    path::Path,
    time::Duration,
};

pub use notify::{
    Error,
    Event,
    RecursiveMode,
};
use notify::{
    RecommendedWatcher,
    Watcher,
};
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct WatchFiles {
    watcher: RecommendedWatcher,
    event_rx: mpsc::Receiver<Event>,
}

impl WatchFiles {
    pub fn new() -> notify::Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(16);

        // note: the watcher is shutdown when it's dropped.
        let watcher = notify::recommended_watcher(move |result: Result<Event, Error>| {
            if let Ok(event) = result {
                let _ = event_tx.blocking_send(event);
            }
        })?;

        Ok(Self { watcher, event_rx })
    }

    pub fn watch(
        &mut self,
        path: impl AsRef<Path>,
        recursive_mode: RecursiveMode,
    ) -> Result<(), Error> {
        self.watcher.watch(path.as_ref(), recursive_mode)?;
        Ok(())
    }

    pub async fn next_event(&mut self) -> Result<Event, Closed> {
        self.event_rx.recv().await.ok_or(Closed)
    }
}

#[derive(Debug)]
pub struct WatchModified {
    watch: WatchFiles,
    debounce: Option<Duration>,
}

impl WatchModified {
    pub fn new(watch: WatchFiles) -> Result<Self, Error> {
        Ok(Self {
            watch,
            debounce: None,
        })
    }

    pub fn with_debounce(mut self, debounce: Duration) -> Self {
        self.debounce = Some(debounce);
        self
    }

    pub async fn modified(&mut self) -> Result<(), Closed> {
        pub async fn modified(watch: &mut WatchFiles) -> Result<(), Closed> {
            loop {
                if watch.next_event().await?.kind.is_modify() {
                    return Ok(());
                }
            }
        }

        modified(&mut self.watch).await?;

        if let Some(debounce) = self.debounce {
            loop {
                match tokio::time::timeout(debounce, modified(&mut self.watch)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(Closed)) => return Err(Closed),
                    Err(_) => return Ok(()),
                }
            }
        }
        else {
            Ok(())
        }
    }
}

pub fn watch_modified(path: impl AsRef<Path>, debounce: Duration) -> Result<WatchModified, Error> {
    let mut watcher = WatchFiles::new()?;
    watcher.watch(path, RecursiveMode::Recursive)?;
    Ok(WatchModified::new(watcher)?.with_debounce(debounce))
}

#[derive(Debug)]
pub struct Closed;
