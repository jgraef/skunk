use leptos::{
    component,
    view,
    IntoView,
};
use leptos_router::{
    Outlet,
    Redirect,
    Route,
    A,
};
use lipsum::lipsum;

use crate::components::icon::{
    BootstrapIcon,
    SkunkIcon,
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
