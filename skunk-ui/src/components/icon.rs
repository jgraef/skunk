use leptos::{
    component,
    view,
    IntoView,
    MaybeSignal,
    Oco,
    SignalGet,
};

#[component]
pub fn BootstrapIcon(
    #[prop(into)] icon: MaybeSignal<String>,
    #[prop(into, optional)] alt: Option<Oco<'static, str>>,
) -> impl IntoView {
    view! { <i class={move || format!("bi bi-{}", icon.get())} aria-label=alt></i> }
}
