use leptos::{
    component,
    create_rw_signal,
    view,
    For,
    IntoView,
};

use crate::components::expand_button::ExpandButton;

stylance::import_crate_style!(style, "src/app/flows.module.scss");

#[component]
pub fn Flows() -> impl IntoView {
    view! {
        <div class=style::flows>
            <table>
                <thead>
                    <tr>
                        <th scope="col"></th>
                        <th scope="col">"Timestamp"</th>
                        <th scope="col">"Protocol"</th>
                        <th scope="col">"Source"</th>
                        <th scope="col">"Destination"</th>
                        <th scope="col">"Info"</th>
                    </tr>
                </thead>
                <tbody>
                    <For
                        each=move || (0..10).into_iter()
                        key=|x| *x
                        children=move |i| {
                            let expanded = create_rw_signal(false);
                            view! {
                                <tr class=style::entry>
                                    <td><ExpandButton expanded /></td>
                                    <td>"Fri Jul  5 06:47:49 AM CEST 2024"</td>
                                    <td>"https"</td>
                                    <td>"localhost:12345"</td>
                                    <td>"maia.crimew.gay:443"</td>
                                    <td>"GET https://maia.crimew.gay/posts/" {i}</td>
                                </tr>
                                <tr
                                    class=style::info
                                >
                                    <td colspan="6">
                                        <div class=style::expander data-expanded=expanded>
                                            <div class=style::expander_content>
                                                "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum."
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                            }
                        }
                    />
                </tbody>
            </table>
        </div>
    }
}
