use dioxus::prelude::*;
use wasm_bindgen::JsCast;

// const FAVICON: Asset = asset!("/assets/favicon.ico");
// const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        // document::Link { rel: "icon", href: FAVICON }
        // document::Link { rel: "stylesheet", href: MAIN_CSS }
        DraggableCircle {}
    }
}

#[component]
fn DraggableCircle() -> Element {
    let mut position = use_signal(|| (100.0, 100.0));
    let mut is_dragging = use_signal(|| false);
    let mut is_resizing = use_signal(|| false);
    let mut drag_offset = use_signal(|| (0.0, 0.0));
    let mut video_ref = use_signal(|| None::<web_sys::HtmlVideoElement>);
    let mut size = use_signal(|| (160.0, 120.0)); // Default size
    let mut resize_start = use_signal(|| (0.0, 0.0));
    let mut resize_corner = use_signal(|| "");
    let mut hover_zone = use_signal(|| "drag"); // "drag" or "resize"
    let mut screen_stream = use_signal(|| None::<web_sys::MediaStream>);
    let mut is_screen_sharing = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                if let Ok(media_devices) = navigator.media_devices() {
                    // Create constraints for video
                    let constraints = web_sys::MediaStreamConstraints::new();
                    constraints.set_video(&true.into());
                    constraints.set_audio(&false.into());

                    // Get user media
                    if let Ok(promise) = media_devices.get_user_media_with_constraints(&constraints)
                    {
                        let future = wasm_bindgen_futures::JsFuture::from(promise);
                        if let Ok(stream) = future.await {
                            if let Ok(media_stream) = stream.dyn_into::<web_sys::MediaStream>() {
                                // Set the stream to video element if we have a reference
                                if let Some(video_elem) = video_ref() {
                                    video_elem.set_src_object(Some(&media_stream));
                                    let _ = video_elem.play();
                                }
                            }
                        }
                    }
                }
            }
        });
    });

    // Function to start screen sharing

    rsx! {
        div {
            style: "position: relative; width: 100vw; height: 100vh; background-color: #0f1116;",
            onmousemove: move |event| {
                let mouse_x = event.client_coordinates().x;
                let mouse_y = event.client_coordinates().y;

                if is_dragging() {
                    let offset = drag_offset();
                    let new_x = mouse_x - offset.0;
                    let new_y = mouse_y - offset.1;

                    // Get viewport dimensions
                    let window = web_sys::window().unwrap();
                    let viewport_width = window.inner_width().unwrap().as_f64().unwrap();
                    let viewport_height = window.inner_height().unwrap().as_f64().unwrap();

                    let current_size = size();

                    // Clamp position to keep box within viewport
                    let clamped_x = new_x.max(0.0).min(viewport_width - current_size.0);
                    let clamped_y = new_y.max(0.0).min(viewport_height - current_size.1);

                    position.set((clamped_x, clamped_y));
                } else if is_resizing() {
                    let _start = resize_start();
                    let corner = resize_corner();
                    let pos = position();
                    let current_size = size();

                    let mut new_width = current_size.0;
                    let mut new_height = current_size.1;

                    // Calculate new dimensions based on which corner is being dragged
                    if corner.contains("right") {
                        new_width = (mouse_x - pos.0).max(160.0).min(400.0);
                    }
                    if corner.contains("bottom") {
                        new_height = (mouse_y - pos.1).max(120.0).min(300.0);
                    }

                    // Maintain aspect ratio (4:3)
                    if corner == "bottom-right" {
                        let aspect_ratio = 4.0 / 3.0;
                        new_height = new_width / aspect_ratio;
                    }

                    size.set((new_width, new_height));
                }
            },
            onmouseup: move |_| {
                is_dragging.set(false);
                is_resizing.set(false);
            },
            onmouseleave: move |_| {
                is_dragging.set(false);
                is_resizing.set(false);
            },

            // Screen share video background
            {
                if let Some(stream) = screen_stream() {
                    rsx! {
                        video {
                            style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; object-fit: contain; z-index: 0;",
                            autoplay: "true",
                            playsinline: "true",
                            muted: "true",
                            onmounted: move |_| {
                                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                                    if let Some(element) = document.query_selector("video[style*='z-index: 0']").ok().flatten() {
                                        if let Ok(video_elem) = element.dyn_into::<web_sys::HtmlVideoElement>() {
                                            video_elem.set_src_object(Some(&stream));
                                            let _ = video_elem.play();
                                        }
                                    }
                                }
                            },
                        }
                    }
                } else {
                    rsx! {}
                }
            }

            // Screen share button
            button {
                style: "position: absolute; top: 20px; left: 20px; z-index: 10; padding: 10px 20px; background-color: #3b82f6; color: white; border: none; border-radius: 8px; cursor: pointer; font-size: 16px; transition: background-color 0.2s;",
                onclick: move |_| {
                    if is_screen_sharing() {
                        // Stop screen share
                        if let Some(stream) = screen_stream() {
                            let tracks = stream.get_tracks();
                            for i in 0..tracks.length() {
                                let track = tracks.get(i);
                                if let Ok(media_track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                                    media_track.stop();
                                }
                            }
                        }
                        screen_stream.set(None);
                        is_screen_sharing.set(false);
                    } else {
                        // Start screen share
                        spawn(async move {
                            if let Some(window) = web_sys::window() {
                                let navigator = window.navigator();
                                if let Ok(media_devices) = navigator.media_devices() {
                                    let constraints = web_sys::MediaStreamConstraints::new();
                                    constraints.set_video(&wasm_bindgen::JsValue::from(true));
                                    constraints.set_audio(&wasm_bindgen::JsValue::from(false));
                                    
                                    if let Ok(promise) = media_devices.get_display_media() {
                                        let future = wasm_bindgen_futures::JsFuture::from(promise);
                                        if let Ok(stream) = future.await {
                                            if let Ok(media_stream) = stream.dyn_into::<web_sys::MediaStream>() {
                                                screen_stream.set(Some(media_stream));
                                                is_screen_sharing.set(true);
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                },
                {if is_screen_sharing() { "Stop Sharing" } else { "Share Screen" }}
            },

            div {
                class: "draggable-circle",
                style: format!("left: {}px; top: {}px; width: {}px; height: {}px; cursor: {}; z-index: 100;", 
                    position().0, 
                    position().1, 
                    size().0, 
                    size().1,
                    if is_dragging() { "grabbing" } else if is_resizing() { "nwse-resize" } else if hover_zone() == "resize" { "nwse-resize" } else { "grab" }
                ),
                onmousemove: move |event| {
                    let mouse_x = event.client_coordinates().x;
                    let mouse_y = event.client_coordinates().y;
                    let pos = position();
                    let current_size = size();

                    // Check if mouse is in corner zone
                    let corner_size = 20.0;
                    let rel_x = mouse_x - pos.0;
                    let rel_y = mouse_y - pos.1;

                    let near_right = rel_x > current_size.0 - corner_size;
                    let near_bottom = rel_y > current_size.1 - corner_size;

                    if near_right && near_bottom {
                        hover_zone.set("resize");
                    } else {
                        hover_zone.set("drag");
                    }
                },
                onmouseleave: move |_| {
                    hover_zone.set("drag");
                },
                onmousedown: move |event| {
                    let mouse_x = event.client_coordinates().x;
                    let mouse_y = event.client_coordinates().y;
                    let pos = position();
                    let current_size = size();

                    // Check if click is in corner zones (20px from edges)
                    let corner_size = 20.0;
                    let rel_x = mouse_x - pos.0;
                    let rel_y = mouse_y - pos.1;

                    let near_right = rel_x > current_size.0 - corner_size;
                    let near_bottom = rel_y > current_size.1 - corner_size;

                    if near_right && near_bottom {
                        is_resizing.set(true);
                        resize_corner.set("bottom-right");
                        resize_start.set((mouse_x, mouse_y));
                    } else {
                        is_dragging.set(true);
                        drag_offset.set((mouse_x - pos.0, mouse_y - pos.1));
                    }
                    event.stop_propagation();
                },
                video {
                    id: "camera-video",
                    style: "width: 100%; height: 100%; object-fit: cover; border-radius: 12px; transform: scaleX(-1);",
                    autoplay: "true",
                    playsinline: "true", 
                    muted: "true",
                    onmounted: move |_| {
                        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                            if let Some(element) = document.get_element_by_id("camera-video") {
                                if let Ok(video_elem) = element.dyn_into::<web_sys::HtmlVideoElement>() {
                                    video_ref.set(Some(video_elem));
                                }
                            }
                        }
                    },
                }
                // Resize handle
                div {
                    style: "position: absolute; bottom: 0; right: 0; width: 20px; height: 20px; cursor: nwse-resize; background: linear-gradient(135deg, transparent 50%, rgba(255,255,255,0.2) 50%); border-bottom-right-radius: 12px;",
                }
            }
        }
    }
}
