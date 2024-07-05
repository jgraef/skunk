use leptos::SignalUpdate;

pub trait SignalToggle {
    fn toggle(&self);
}

impl<T: SignalUpdate<Value = bool>> SignalToggle for T {
    fn toggle(&self) {
        self.update(|value| *value = !*value);
    }
}
