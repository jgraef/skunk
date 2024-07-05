use leptos::{
    component,
    view,
    IntoView,
};

use crate::components::icon::BootstrapIcon;

stylance::import_crate_style!(style, "src/app/home.module.scss");

pub const GITHUB_LINK: &'static str = "https://github.com/jgraef/skunk";
pub const DISCORD_LINK: &'static str = "https://discord.gg/skunk-todo";
pub const TWITTER_LINK: &'static str = "https://twitter.com/skunk-todo";
pub const REDDIT_LINK: &'static str = "https://reddit.com/r/skunk-proxy";

#[component]
pub fn Home() -> impl IntoView {
    view! {
        <div class=style::welcome>
            <img src="NotoSkunk.svg" />

            <h1>"Welcome to skunk"</h1>

            <div class=style::socials>
                <a href=GITHUB_LINK target="_blank">
                    <BootstrapIcon icon="github" />
                </a>
                <a href=DISCORD_LINK target="_blank">
                    <BootstrapIcon icon="discord" />
                </a>
                <a href=TWITTER_LINK target="_blank">
                    <BootstrapIcon icon="twitter" />
                </a>
                <a href=REDDIT_LINK target="_blank">
                    <BootstrapIcon icon="reddit" />
                </a>
            </div>

            <div class=style::actions>
                <a href="#">"Start intercepting"</a>
                <a href="#">"Read docs"</a>
                <a href="/settings">"Configure"</a>
                <a href="/settings/tls">"Install root certificate"</a>
            </div>
        </div>
    }
}
