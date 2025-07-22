use dioxus::prelude::*;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        DraggableCircle {}

    }
}

#[component]
fn DraggableCircle() -> Element {
    let mut position = use_signal(|| (100.0, 100.0));
    let mut is_dragging = use_signal(|| false);
    let mut drag_offset = use_signal(|| (0.0, 0.0));

    rsx! {
        div {
            style: "position: relative; width: 100vw; height: 100vh;",
            onmousemove: move |event| {
                if is_dragging() {
                    let offset = drag_offset();
                    let new_x = event.client_coordinates().x - offset.0;
                    let new_y = event.client_coordinates().y - offset.1;
                    
                    // Get viewport dimensions
                    let window = web_sys::window().unwrap();
                    let viewport_width = window.inner_width().unwrap().as_f64().unwrap();
                    let viewport_height = window.inner_height().unwrap().as_f64().unwrap();
                    
                    // Circle diameter is 80px (from CSS)
                    let circle_size = 80.0;
                    
                    // Clamp position to keep circle within viewport
                    let clamped_x = new_x.max(0.0).min(viewport_width - circle_size);
                    let clamped_y = new_y.max(0.0).min(viewport_height - circle_size);
                    
                    position.set((clamped_x, clamped_y));
                }
            },
            onmouseup: move |_| {
                is_dragging.set(false);
            },
            onmouseleave: move |_| {
                is_dragging.set(false);
            },
            
            div {
                class: "draggable-circle",
                style: "left: {position().0}px; top: {position().1}px;",
                onmousedown: move |event| {
                    is_dragging.set(true);
                    let circle_pos = position();
                    drag_offset.set((
                        event.client_coordinates().x - circle_pos.0,
                        event.client_coordinates().y - circle_pos.1
                    ));
                    event.stop_propagation();
                },
            }
        }
    }
}
