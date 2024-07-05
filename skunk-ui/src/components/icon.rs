use leptos::{
    component,
    view,
    IntoView,
    MaybeSignal,
    Oco,
    SignalGet,
};

stylance::import_crate_style!(style, "src/components/icon.module.scss");

#[component]
pub fn BootstrapIcon(
    #[prop(into)] icon: MaybeSignal<String>,
    #[prop(into, optional)] alt: Option<Oco<'static, str>>,
) -> impl IntoView {
    view! { <i class={move || format!("bi bi-{}", icon.get())} aria-label=alt></i> }
}

#[component]
pub fn SkunkIcon() -> impl IntoView {
    view! {
        <img src="NotoSkunk.svg" class=style::skunk_icon />
    }
}
