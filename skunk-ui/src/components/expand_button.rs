use leptos::{
    component,
    view,
    IntoView,
    RwSignal,
    Signal,
};

use super::icon::BootstrapIcon;
use crate::util::SignalToggle;

stylance::import_crate_style!(style, "src/components/expand_button.module.scss");

#[component]
pub fn ExpandButton(expanded: RwSignal<bool>) -> impl IntoView {
    view! {
        <button
            class=style::expand_button
            type="button"
            on:click=move |_| {
                expanded.toggle();
            }
        >
            <ExpandIcon expanded={Signal::from(expanded)}/>
        </button>
    }
}

#[component]
pub fn ExpandIcon(expanded: Signal<bool>) -> impl IntoView {
    view! {
        <span class=style::expand_icon data-expanded=expanded>
            <BootstrapIcon icon="caret-right-fill" />
        </span>
    }
}
