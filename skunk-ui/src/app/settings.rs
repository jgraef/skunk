use leptos::{
    component,
    event_target_value,
    view,
    IntoView,
};
use leptos_router::{
    Outlet,
    Redirect,
    Route,
    A,
};
use leptos_use::ColorMode;
use lipsum::lipsum;

use crate::{
    app::Context,
    components::icon::BootstrapIcon,
};

stylance::import_crate_style!(style, "src/app/settings.module.scss");

#[component(transparent)]
pub fn SettingsRoutes() -> impl IntoView {
    view! {
        <Route path="/settings" view=Settings>
            <Route path="general" view=General />
            <Route path="tls" view=Tls />
            <Route path="" view=|| view!{ <Redirect path="/settings/general" /> } />
        </Route>
    }
}

#[component]
fn Settings() -> impl IntoView {
    view! {
        <div class=style::settings>
            <div class=style::sidebar>
                <ul>
                    <li>
                        <A href="/settings/general">
                            <BootstrapIcon icon="gear" />
                            "General"
                        </A>
                    </li>
                    <li>
                        <A href="/settings/tls">
                            <BootstrapIcon icon="shield-lock" />
                            "TLS"
                        </A>
                    </li>
                </ul>
            </div>
            <Outlet />
        </div>
    }
}

#[component]
fn General() -> impl IntoView {
    view! {
        <div class=super::style::page>
            <h1>
                <BootstrapIcon icon="gear"/>
                "General"
            </h1>
            <p>
                { lipsum(100) }
            </p>
            <h2>
                <BootstrapIcon icon="brush" />
                "Theme"
            </h2>
            <select
                class="form-select"
                aria-label="Select theme"
                on:change=move |event| {
                    let value = event_target_value(&event);
                    let mode = match value.as_str() {
                        "system-default" => ColorMode::Auto,
                        "light" => ColorMode::Light,
                        "dark" => ColorMode::Dark,
                        _ => {
                            tracing::warn!("Invalid theme selected: {value}");
                            return;
                        },
                    };

                    let Context { theme, .. } = Context::get();
                    theme.set(mode);
                }
            >
                <option value="system-default">
                    "System default"
                </option>
                <option value="light">
                    <BootstrapIcon icon="sun-fill" />
                    "Light"
                </option>
                <option value="dark">
                    <BootstrapIcon icon="moon-stars-fill" />
                    "Dark"
                </option>
            </select>
        </div>
    }
}

#[component]
fn Tls() -> impl IntoView {
    view! {
        <div class=super::style::page>
            <h1>
                <BootstrapIcon icon="shield-lock"/>
                "Transport Layer Security (TLS)"
            </h1>
            <p>
                { lipsum(100) }
            </p>
            <h2>"Root certificate"</h2>
            <p>
                "Even skunk can't break TLS encryption :( At least not without your help :3c" <br />
                "You'll have to install skunk's root certificate on the device you want to monitor. This allows skunk to decrypt its traffic. This certificate is generated for your install, so only you can decrypt your traffic."
            </p>
            <ul>
                <li>"Linux: " <a href="/api/settings/tls/ca.cert.pem">"ca.cert.pem"</a></li>
                <li>"Android: " <a href="/api/settings/tls/ca.cert.crt">"ca.cert.crt"</a></li>
            </ul>
        </div>
    }
}
