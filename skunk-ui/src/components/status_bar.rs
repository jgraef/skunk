use leptos::{
    component,
    view,
    IntoView,
    Signal,
    SignalGet,
};
use skunk_api::client::Status;

use crate::{
    app::Context,
    components::icon::BootstrapIcon,
    util::WatchExt,
};

stylance::import_crate_style!(style, "src/components/status_bar.module.scss");

#[component]
pub fn StatusBar() -> impl IntoView {
    let Context { client, .. } = Context::get();
    let status = client.status().into_signal();
    let status_icon = Signal::derive(move || {
        match status.get() {
            Status::Disconnected => "x-lg",
            Status::Connected => "check-lg",
        }
        .to_owned()
    });

    view! {
        <div class=style::status_bar>
            <BootstrapIcon icon="plug-fill" />
            <BootstrapIcon icon=status_icon />
        </div>
    }
}
