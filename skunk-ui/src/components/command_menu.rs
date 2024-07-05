use leptos::{
    component,
    create_node_ref,
    create_rw_signal,
    event_target_value,
    html::Input,
    view,
    For,
    IntoView,
    Oco,
    Signal,
    SignalGet,
    SignalSet,
};
use leptos_hotkeys::use_hotkeys;
use lipsum::lipsum_title_with_rng;
use rand::{
    seq::SliceRandom,
    thread_rng,
};

use crate::components::icon::BootstrapIcon;

stylance::import_crate_style!(style, "src/components/command_menu.module.scss");

#[component]
pub fn CommandMenu() -> impl IntoView {
    let active = create_rw_signal(false);
    let commands = CommandProvider::default();
    let input_ref = create_node_ref::<Input>();
    let suggestions = create_rw_signal(commands.suggest(""));
    let selected = create_rw_signal(None);

    use_hotkeys!(("Control+keyP") => move |_| {
        active.set(true);

        let input = input_ref.get_untracked().unwrap();
        let _ = input.focus();
    });

    let close = move || {
        let input = input_ref.get_untracked().unwrap();

        let value = input.value();
        input.set_value("");

        active.set(false);
        suggestions.set(vec![]);
        selected.set(None);

        value
    };

    view! {
        <div class=style::menu data-active=active>
            <form
                on:submit=move |event| {
                    event.prevent_default();
                    let value = close();
                    tracing::debug!(value, "command entered");
                }
            >
                <input
                    type="text"
                    placeholder="Enter a command (or ?)"
                    value=""
                    node_ref=input_ref
                    on:input=move |event| {
                        let value = event_target_value(&event);
                        suggestions.set(commands.suggest(&value));
                    }
                    on:focusout=move |_| {
                        close();
                    }
                />

                <div class=style::suggestions>
                    <For
                        each=move || suggestions.get()
                        key=|command| command.id
                        children=move |command| {
                            let id = command.id;

                            let is_selected = Signal::derive(move || {
                                selected.get() == Some(id)
                            });

                            view! {
                                <button
                                    data-selected=is_selected
                                    on:click=move |_| {
                                        close();
                                        tracing::debug!(id, "command selected");
                                    }
                                >
                                    <span class=style::icon>
                                        <BootstrapIcon icon=command.icon.into_owned() />
                                    </span>
                                    {command.label}
                                </button>
                            }
                        }
                    />
                </div>
            </form>
        </div>
    }
}

pub struct CommandProvider {
    commands: Vec<Command>,
}

impl Default for CommandProvider {
    fn default() -> Self {
        // mock some commands

        let icons = &[
            "command",
            "terminal",
            "terminal-dash",
            "terminal-plus",
            "terminal-split",
            "terminal-x",
            "ethernet",
            "pci-card-network",
            "router",
            "wifi",
        ];

        let mut rng = thread_rng();

        Self {
            commands: (0..20)
                .into_iter()
                .map(|id| {
                    Command {
                        id,
                        icon: (*icons.choose(&mut rng).unwrap()).into(),
                        label: lipsum_title_with_rng(&mut rng).into(),
                    }
                })
                .collect(),
        }
    }
}

impl CommandProvider {
    pub fn suggest(&self, input: &str) -> Vec<Command> {
        self.commands
            .iter()
            .filter(|command| command.label.contains(input))
            .cloned()
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    id: usize,
    icon: Oco<'static, str>,
    label: Oco<'static, str>,
}

impl Command {
    pub fn new(
        id: usize,
        icon: impl Into<Oco<'static, str>>,
        label: impl Into<Oco<'static, str>>,
    ) -> Self {
        Self {
            id,
            icon: icon.into(),
            label: label.into(),
        }
    }
}
