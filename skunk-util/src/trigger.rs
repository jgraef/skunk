use std::future::pending;

use tokio::sync::watch;

#[derive(Clone, Debug)]
pub struct Sender {
    tx: watch::Sender<()>,
}

impl Sender {
    pub fn trigger(&self) {
        let _ = self.tx.send(());
    }
}

#[derive(Debug)]
pub struct Receiver {
    rx: watch::Receiver<()>,
}

impl Default for Receiver {
    fn default() -> Self {
        let (_tx, rx) = new();
        rx
    }
}

impl Clone for Receiver {
    fn clone(&self) -> Self {
        let mut rx = self.rx.clone();
        rx.mark_unchanged();
        Self { rx }
    }
}

impl Receiver {
    pub async fn triggered_or_closed(&mut self) -> Result<(), Closed> {
        self.rx.changed().await.map_err(|_| Closed)
    }

    pub async fn triggered(&mut self) {
        if let Err(Closed) = self.triggered_or_closed().await {
            pending::<()>().await;
        }
    }
}

#[derive(Debug)]
pub struct Closed;

pub fn new() -> (Sender, Receiver) {
    let (tx, rx) = watch::channel(());
    (Sender { tx }, Receiver { rx })
}
