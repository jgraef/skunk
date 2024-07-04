mod flows;
mod home;

use leptos::{
    component,
    view,
    DynAttrs,
    IntoView,
    Oco,
    Signal,
    SignalGet,
    SignalSet,
    WriteSignal,
};
use leptos_meta::{
    provide_meta_context,
    Html,
};
use leptos_router::{
    Route,
    Router,
    Routes,
    ToHref,
    A,
};
use leptos_use::{
    use_color_mode,
    ColorMode,
    UseColorModeReturn,
};

use self::{
    flows::Flows,
    home::Home,
};

pub const GITHUB_PAGE: &'static str = "https://github.com/jgraef/skunk";

#[component]
pub fn BootstrapIcon(
    #[prop(into)] icon: Oco<'static, str>,
    #[prop(into, optional)] alt: Option<Oco<'static, str>>,
) -> impl IntoView {
    view! { <i class={format!("bi bi-{icon}")} aria-label=alt></i> }
}

#[component]
pub fn NavLink<H: ToHref + 'static>(
    href: H,
    #[prop(into)] icon: Oco<'static, str>,
    #[prop(into)] label: Oco<'static, str>,
) -> impl IntoView {
    view! {
        <li class="nav-item" data-bs-toggle="tooltip" data-bs-placement="right" data-bs-original-title=label.clone()>
            <A href={href} active_class="active" class="nav-link">
                <BootstrapIcon icon=icon alt=label />
            </A>
        </li>
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    bs_theme: Signal<&'static str>,
    theme_icon: Signal<&'static str>,
    mode: Signal<ColorMode>,
    set_mode: WriteSignal<ColorMode>,
}

impl Default for Theme {
    fn default() -> Self {
        let UseColorModeReturn { mode, set_mode, .. } = use_color_mode();
        let bs_theme = Signal::derive(move || {
            match mode.get() {
                ColorMode::Dark => "dark",
                _ => "light",
            }
        });
        let theme_icon = Signal::derive(move || {
            match mode.get() {
                ColorMode::Dark => "moon-stars-fill",
                _ => "sun-fill",
            }
        });
        Self {
            bs_theme,
            theme_icon,
            mode,
            set_mode,
        }
    }
}

impl Theme {
    pub fn toggle(&self) {
        let current = self.mode.get();
        let new = match current {
            ColorMode::Dark => ColorMode::Light,
            _ => ColorMode::Dark,
        };
        self.set_mode.set(new);
    }
}

#[derive(Clone, Debug)]
pub struct Context {
    theme: Theme,
}

impl Context {
    pub fn provide() -> Self {
        let context = Self {
            theme: Theme::default(),
        };
        leptos::provide_context(context.clone());
        context
    }

    pub fn get() -> Self {
        leptos::expect_context()
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let Context {
        theme: Theme { bs_theme, .. },
        ..
    } = Context::provide();

    view! {
        <Html
            attr:data-bs-theme=bs_theme
        />
        <Router>
            <div class="d-flex flex-row" style="height: 100vh; width: 100%">
                <nav class="d-flex flex-column flex-shrink-0 text-white sidebar">
                    <ul class="nav nav-pills nav-flush flex-column mb-auto text-center">
                        <NavLink href="/" icon="house" label="Home" />
                        <NavLink href="/flows" icon="ethernet" label="Flows" />
                        <li class="mt-auto"></li>
                        <NavLink href="/settings" icon="gear" label="Settings" />
                    </ul>
                </nav>
                <main class="main d-flex flex-column w-100 h-100 mw-100 mh-100 position-relative">
                    <Routes>
                        <Route path="/" view=Home />
                        <Route path="/flows" view=Flows />
                        <Route path="/settings" view=|| view!{ "TODO" } />
                        <Route path="/*any" view=NotFound />
                    </Routes>
                </main>
            </div>
        </Router>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <div class="h-100 w-100 pt-3 px-4">
            <h1>"404 - Not found"</h1>
        </div>
    }
}
