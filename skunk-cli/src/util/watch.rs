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
pub struct FileWatcher {
    watcher: RecommendedWatcher,
    event_rx: mpsc::Receiver<Event>,
}

impl FileWatcher {
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

    pub async fn modified(&mut self) -> Result<(), Closed> {
        loop {
            if self.next_event().await?.kind.is_modify() {
                return Ok(());
            }
        }
    }
}

#[derive(Debug)]
pub struct WatchModified {
    watcher: FileWatcher,
    debounce: Duration,
}

impl WatchModified {
    pub fn new(watcher: FileWatcher, debounce: Duration) -> Result<Self, Error> {
        Ok(Self { watcher, debounce })
    }

    pub async fn wait(&mut self) -> Result<(), Closed> {
        self.watcher.modified().await?;

        loop {
            match tokio::time::timeout(self.debounce, self.watcher.modified()).await {
                Ok(Ok(())) => {}
                Ok(Err(Closed)) => return Err(Closed),
                Err(_) => return Ok(()),
            }
        }
    }
}

pub fn watch_modified(path: impl AsRef<Path>, debounce: Duration) -> Result<WatchModified, Error> {
    let mut watcher = FileWatcher::new()?;
    watcher.watch(path, RecursiveMode::Recursive)?;
    WatchModified::new(watcher, debounce)
}

#[derive(Debug)]
pub struct Closed;
