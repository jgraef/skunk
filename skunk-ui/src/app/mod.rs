mod flows;
mod home;
mod settings;

use leptos::{
    component,
    create_node_ref,
    html,
    view,
    DynAttrs,
    IntoView,
    Signal,
    SignalGet,
    SignalSet,
    WriteSignal,
};
use leptos_hotkeys::{
    provide_hotkeys_context,
    scopes,
    HotkeysContext,
};
use leptos_meta::{
    provide_meta_context,
    Html,
};
use leptos_router::{
    Route,
    Router,
    Routes,
};
use leptos_use::{
    use_color_mode,
    ColorMode,
    UseColorModeReturn,
};
use settings::SettingsRoutes;

use self::{
    flows::Flows,
    home::Home,
};
use crate::components::{
    command_menu::CommandMenu,
    dock::Dock,
};

stylance::import_crate_style!(style, "src/app/app.module.scss");

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

    let main_ref = create_node_ref::<html::Main>();
    let HotkeysContext { .. } = provide_hotkeys_context(main_ref, false, scopes!());

    view! {
        <Html
            attr:data-bs-theme=bs_theme
        />
        <Router>
            <div class="d-flex flex-row" style="height: 100vh; width: 100%">
                <Dock />
                <main class=style::main node_ref=main_ref>
                    <CommandMenu />
                    <Routes>
                        <Route path="/" view=Home />
                        <Route path="/flows" view=Flows />
                        <Route path="/filters" view=|| view!{ "TODO" } />
                        <SettingsRoutes />
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
