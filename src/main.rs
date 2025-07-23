use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

// const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        // document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        DraggableCircle {}
    }
}

#[component]
fn DraggableCircle() -> Element {
    let mut position = use_signal(|| (100.0, 100.0));
    let mut is_dragging = use_signal(|| false);
    let mut is_resizing = use_signal(|| false);
    let mut drag_offset = use_signal(|| (0.0, 0.0));
    let mut camera_video_ref = use_signal(|| None::<web_sys::HtmlVideoElement>);
    let mut screen_video_ref = use_signal(|| None::<web_sys::HtmlVideoElement>);
    let mut canvas_ref = use_signal(|| None::<web_sys::HtmlCanvasElement>);
    let mut size = use_signal(|| (160.0, 120.0)); // Default size
    let mut resize_start = use_signal(|| (0.0, 0.0));
    let mut resize_corner = use_signal(|| "");
    let mut screen_stream = use_signal(|| None::<web_sys::MediaStream>);
    let mut is_screen_sharing = use_signal(|| false);
    let mut animation_frame_id = use_signal(|| None::<i32>);
    let mut cursor_state = use_signal(|| "default"); // "default", "grab", "nwse-resize"

    // Function to update cursor based on mouse position
    let mut update_cursor = move |mouse_x: f64, mouse_y: f64| {
        let pos = position();
        let cam_size = size();
        
        // Check if mouse is within camera area
        if mouse_x >= pos.0 && mouse_x <= pos.0 + cam_size.0 &&
           mouse_y >= pos.1 && mouse_y <= pos.1 + cam_size.1 {
            
            // Check if mouse is in resize corner (20px from bottom-right)
            let corner_size = 20.0;
            let rel_x = mouse_x - pos.0;
            let rel_y = mouse_y - pos.1;
            let near_right = rel_x > cam_size.0 - corner_size;
            let near_bottom = rel_y > cam_size.1 - corner_size;
            
            if near_right && near_bottom {
                cursor_state.set("nwse-resize");
            } else {
                cursor_state.set("grab");
            }
        } else {
            cursor_state.set("default");
        }
    };

    // Rendering loop for canvas
    let render_loop = move || {
        if let (Some(canvas), Some(camera_video)) = (canvas_ref(), camera_video_ref()) {
            if let Ok(Some(context)) = canvas.get_context("2d") {
                if let Ok(ctx) = context.dyn_into::<web_sys::CanvasRenderingContext2d>() {
                    let window = web_sys::window().unwrap();
                    let viewport_width = window.inner_width().unwrap().as_f64().unwrap();
                    let viewport_height = window.inner_height().unwrap().as_f64().unwrap();
                    
                    // Set canvas size
                    canvas.set_width(viewport_width as u32);
                    canvas.set_height(viewport_height as u32);
                    
                    // Clear canvas
                    ctx.clear_rect(0.0, 0.0, viewport_width, viewport_height);
                    
                    // Draw screen share if active (scaled to fill canvas with letterboxing)
                    if let Some(screen_video) = screen_video_ref() {
                        if is_screen_sharing() && screen_video.ready_state() >= 2 {
                            let video_width = screen_video.video_width() as f64;
                            let video_height = screen_video.video_height() as f64;
                            
                            if video_width > 0.0 && video_height > 0.0 {
                                // Calculate scale to fit canvas while maintaining aspect ratio (letterboxing)
                                let scale_x = viewport_width / video_width;
                                let scale_y = viewport_height / video_height;
                                let scale = scale_x.min(scale_y); // Use min to fit (letterbox if needed)
                                
                                let scaled_width = video_width * scale;
                                let scaled_height = video_height * scale;
                                
                                // Center the video on the canvas
                                let x = (viewport_width - scaled_width) / 2.0;
                                let y = (viewport_height - scaled_height) / 2.0;
                                
                                let _ = ctx.draw_image_with_html_video_element_and_dw_and_dh(
                                    &screen_video,
                                    x,
                                    y, 
                                    scaled_width,
                                    scaled_height
                                );
                            }
                        }
                    }
                    
                    // Draw camera overlay
                    if camera_video.ready_state() >= 2 {
                        let pos = position();
                        let cam_size = size();
                        
                        // Save context state
                        ctx.save();
                        
                        // Clip to rounded rectangle
                        ctx.begin_path();
                        ctx.move_to(pos.0 + 12.0, pos.1);
                        ctx.line_to(pos.0 + cam_size.0 - 12.0, pos.1);
                        ctx.quadratic_curve_to(pos.0 + cam_size.0, pos.1, pos.0 + cam_size.0, pos.1 + 12.0);
                        ctx.line_to(pos.0 + cam_size.0, pos.1 + cam_size.1 - 12.0);
                        ctx.quadratic_curve_to(pos.0 + cam_size.0, pos.1 + cam_size.1, pos.0 + cam_size.0 - 12.0, pos.1 + cam_size.1);
                        ctx.line_to(pos.0 + 12.0, pos.1 + cam_size.1);
                        ctx.quadratic_curve_to(pos.0, pos.1 + cam_size.1, pos.0, pos.1 + cam_size.1 - 12.0);
                        ctx.line_to(pos.0, pos.1 + 12.0);
                        ctx.quadratic_curve_to(pos.0, pos.1, pos.0 + 12.0, pos.1);
                        ctx.close_path();
                        ctx.clip();
                        
                        // Flip horizontally for mirror effect
                        let _ = ctx.translate(pos.0 + cam_size.0 / 2.0, pos.1 + cam_size.1 / 2.0);
                        let _ = ctx.scale(-1.0, 1.0);
                        let _ = ctx.translate(-(pos.0 + cam_size.0 / 2.0), -(pos.1 + cam_size.1 / 2.0));
                        
                        // Draw camera video
                        let _ = ctx.draw_image_with_html_video_element_and_dw_and_dh(
                            &camera_video,
                            pos.0,
                            pos.1,
                            cam_size.0,
                            cam_size.1
                        );
                        
                        // Restore context state
                        ctx.restore();
                    }
                }
            }
        }
    };

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
                                if let Some(video_elem) = camera_video_ref() {
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

    // Start render loop with high-frequency setInterval (smoother than before)
    use_effect(move || {
        let window = web_sys::window().unwrap();
        let render_loop_clone = render_loop.clone();
        
        let closure = Closure::wrap(Box::new(move || {
            render_loop_clone();
        }) as Box<dyn Fn()>);
        
        // 60fps = ~16.67ms, using 16ms for smoother rendering
        let id = window.set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            16
        ).unwrap();
        
        animation_frame_id.set(Some(id));
        
        // Leak closure to keep it alive
        closure.forget();
    });

    // Function to start screen sharing

    rsx! {
        div {
            style: "position: relative; width: 100vw; height: 100vh; background-color: #0f1116;",
            
            // Canvas for rendering both videos
            canvas {
                id: "main-canvas",
                style: format!("position: absolute; top: 0; left: 0; width: 100%; height: 100%; cursor: {};", cursor_state()),
                onmousemove: move |event| {
                    let mouse_x = event.client_coordinates().x;
                    let mouse_y = event.client_coordinates().y;

                    if is_dragging() {
                        cursor_state.set("grabbing");
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
                        cursor_state.set("nwse-resize");
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
                    } else {
                        // Update cursor based on mouse position when not dragging/resizing
                        update_cursor(mouse_x, mouse_y);
                    }
                },
                onmousedown: move |event| {
                    let mouse_x = event.client_coordinates().x;
                    let mouse_y = event.client_coordinates().y;
                    let pos = position();
                    let current_size = size();

                    // Check if click is within camera area
                    if mouse_x >= pos.0 && mouse_x <= pos.0 + current_size.0 &&
                       mouse_y >= pos.1 && mouse_y <= pos.1 + current_size.1 {
                        
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
                    }
                },
                onmouseup: move |event| {
                    is_dragging.set(false);
                    is_resizing.set(false);
                    // Update cursor based on final mouse position
                    let mouse_x = event.client_coordinates().x;
                    let mouse_y = event.client_coordinates().y;
                    update_cursor(mouse_x, mouse_y);
                },
                onmouseleave: move |_| {
                    is_dragging.set(false);
                    is_resizing.set(false);
                    cursor_state.set("default");
                },
                onmounted: move |_| {
                    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                        if let Some(element) = document.get_element_by_id("main-canvas") {
                            if let Ok(canvas_elem) = element.dyn_into::<web_sys::HtmlCanvasElement>() {
                                canvas_ref.set(Some(canvas_elem));
                            }
                        }
                    }
                },
            }
            
            // Hidden video elements
            video {
                id: "camera-video",
                style: "display: none;",
                autoplay: "true",
                playsinline: "true",
                muted: "true",
                onmounted: move |_| {
                    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                        if let Some(element) = document.get_element_by_id("camera-video") {
                            if let Ok(video_elem) = element.dyn_into::<web_sys::HtmlVideoElement>() {
                                camera_video_ref.set(Some(video_elem));
                            }
                        }
                    }
                },
            }
            
            video {
                id: "screen-video",
                style: "display: none;",
                autoplay: "true",
                playsinline: "true",
                muted: "true",
                onmounted: move |_| {
                    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                        if let Some(element) = document.get_element_by_id("screen-video") {
                            if let Ok(video_elem) = element.dyn_into::<web_sys::HtmlVideoElement>() {
                                screen_video_ref.set(Some(video_elem));
                            }
                        }
                    }
                },
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
                                                // Set the stream to screen video element
                                                if let Some(video_elem) = screen_video_ref() {
                                                    video_elem.set_src_object(Some(&media_stream));
                                                    let _ = video_elem.play();
                                                }
                                                screen_stream.set(Some(media_stream.clone()));
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
        }
    }
}
