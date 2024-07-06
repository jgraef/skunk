use leptos::{
    create_signal,
    ReadSignal,
    Signal,
    SignalSet,
    SignalUpdate,
};
use tokio::sync::watch;

pub trait SignalToggle {
    fn toggle(&self);
}

impl<T: SignalUpdate<Value = bool>> SignalToggle for T {
    fn toggle(&self) {
        self.update(|value| *value = !*value);
    }
}

pub trait WatchExt<T> {
    fn into_signal(self) -> ReadSignal<T>;
}

impl<T: Clone> WatchExt<T> for watch::Receiver<T> {
    fn into_signal(mut self) -> ReadSignal<T> {
        let value = self.borrow().clone();
        let (signal, set_signal) = create_signal(value);
        leptos::spawn_local(async move {
            while let Ok(()) = self.changed().await {
                let value = self.borrow().clone();
                set_signal.set(value);
            }
        });
        signal
    }
}
