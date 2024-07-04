use leptos::{
    component,
    view,
    IntoView,
    SignalGet,
};

use super::{
    BootstrapIcon,
    Context,
    GITHUB_PAGE,
};

#[component]
pub fn Home() -> impl IntoView {
    let Context { theme, .. } = Context::get();

    view! {
        <h1>
            <i>"skunk"</i> "&mdash;" <img src="/NotoSkunk.svg" style="height: 1em;" /> "A person-in-the-middle proxy"
            <small class="d-flex flex-row">
                <button type="button" class="btn py-0 px-1 m-auto" style="color: white;" on:click=move |_| theme.toggle()>
                    {move || {
                        view!{<BootstrapIcon icon=theme.theme_icon.get() />}
                    }}
                </button>
                <a href=GITHUB_PAGE target="_blank" class="py-0 px-1 m-auto" style="color: white;">
                    <BootstrapIcon icon="github" />
                </a>
            </small>
        </h1>
    }
}
