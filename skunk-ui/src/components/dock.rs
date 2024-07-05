use leptos::{
    component,
    view,
    IntoView,
    Oco,
};
use leptos_router::{
    ToHref,
    A,
};

use crate::app::BootstrapIcon;

stylance::import_crate_style!(style, "src/components/dock.module.scss");

#[component]
pub fn Item<H: ToHref + 'static>(
    href: H,
    #[prop(into)] icon: Oco<'static, str>,
    #[prop(into)] label: Oco<'static, str>,
) -> impl IntoView {
    view! {
        <li class=style::item data-bs-toggle="tooltip" data-bs-placement="right" data-bs-original-title=label.clone()>
            <A href={href} active_class="active" class=style::link>
                <BootstrapIcon icon=icon alt=label />
            </A>
        </li>
    }
}

#[component]
pub fn Dock() -> impl IntoView {
    view! {
        <nav class=style::dock>
            <ul class=style::group_top>
                <Item href="/" icon="house" label="Home" />
                <Item href="/flows" icon="ethernet" label="Flows" />
            </ul>
            <ul class=style::group_bottom>
                <Item href="/settings" icon="gear" label="Settings" />
            </ul>
        </nav>
    }
}
