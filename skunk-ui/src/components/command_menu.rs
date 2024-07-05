use leptos::{
    component,
    create_rw_signal,
    view,
    IntoView,
    SignalGet,
    SignalSet,
};
use leptos_hotkeys::use_hotkeys;

stylance::import_crate_style!(style, "src/components/command_menu.module.scss");

#[component]
pub fn CommandMenu() -> impl IntoView {
    let active = create_rw_signal(false);

    use_hotkeys!(("Control+keyP") => move |_| {
        active.set(true);
    });

    view! {
        {move || {
            if active.get() {
                view! {
                    <div class=style::menu>
                        <input type="text" placeholder="Enter a command (or ?)" value="" />
                        <div class=style::suggestions>
                            <button>"Foo Command"</button>
                            <button>"Bar Command"</button>
                        </div>
                    </div>
                }.into_view()
            }
            else {
                view!{}.into_view()
            }
        }}
    }
}
