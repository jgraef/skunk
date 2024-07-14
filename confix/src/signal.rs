use std::ops::Deref;

use tokio::sync::watch;

pub fn signal<T>(initial_value: T) -> (WriteSignal<T>, ReadSignal<T>) {
    let (tx, rx) = watch::channel(initial_value);
    (WriteSignal { inner: tx }, ReadSignal { inner: rx })
}

#[derive(Debug)]
pub struct ReadSignal<T> {
    inner: watch::Receiver<T>,
}

impl<T> ReadSignal<T> {
    pub fn get(&mut self) -> Ref<'_, T> {
        Ref {
            inner: self.inner.borrow_and_update(),
        }
    }

    pub fn peek(&self) -> Ref<'_, T> {
        Ref {
            inner: self.inner.borrow(),
        }
    }

    pub async fn changed(&mut self) -> Result<(), NoWriters> {
        self.inner.changed().await.map_err(|_| NoWriters)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("No more writers for this signal")]
pub struct NoWriters;

pub struct WriteSignal<T> {
    inner: watch::Sender<T>,
}

impl<T> WriteSignal<T> {
    pub fn modify(&self, f: impl FnOnce(&mut T)) {
        self.inner.send_modify(f);
    }

    pub fn replace(&self, new_value: T) {
        self.inner.send_replace(new_value);
    }

    pub fn read_signal(&self) -> ReadSignal<T> {
        ReadSignal {
            inner: self.inner.subscribe(),
        }
    }
}

pub struct Ref<'a, T> {
    inner: watch::Ref<'a, T>,
}

impl<'a, T> Ref<'a, T> {
    pub fn changed(&self) -> bool {
        self.inner.has_changed()
    }
}

impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}
