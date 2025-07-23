use dioxus::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

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
    let mut is_recording = use_signal(|| false);
    let mut media_recorder = use_signal(|| None::<web_sys::MediaRecorder>);
    let mut recorded_chunks = use_signal(|| Vec::<web_sys::Blob>::new());
    let mut camera_enabled = use_signal(|| false);
    let mut camera_stream = use_signal(|| None::<web_sys::MediaStream>);
    let mut is_crop_mode = use_signal(|| false);
    let mut crop_start = use_signal(|| (0.0, 0.0));
    let mut crop_end = use_signal(|| (0.0, 0.0));
    let mut is_drawing_crop = use_signal(|| false);
    let mut crop_bounds = use_signal(|| None::<(f64, f64, f64, f64)>); // (x, y, width, height) in screen video coordinates
    let mut show_chrome_warning = use_signal(|| false);
    let mut is_chrome = use_signal(|| false);
    let mut file_handle = use_signal(|| None::<web_sys::FileSystemFileHandle>);
    let mut file_stream = use_signal(|| None::<web_sys::FileSystemWritableFileStream>);
    let mut is_pointer_mode = use_signal(|| false);
    let mut pointer_position = use_signal(|| (0.0, 0.0));
    let mut camera_zoom = use_signal(|| 1.0); // 1.0 = normal, >1.0 = zoomed in
    let mut pip_video_element = use_signal(|| None::<web_sys::HtmlVideoElement>);
    let mut is_pip_active = use_signal(|| false);
    let mut countdown_value = use_signal(|| 0); // 0 = no countdown, 3,2,1 = countdown values

    // Check if browser is Chrome and File System Access API is supported
    use_effect(move || {
        if let Some(window) = web_sys::window() {
            if let Some(navigator) = window.navigator().user_agent().ok() {
                let is_chrome_browser = navigator.contains("Chrome") && !navigator.contains("Edg");
                let has_file_system_api =
                    js_sys::Reflect::has(&window, &"showSaveFilePicker".into()).unwrap_or(false);

                let chrome_and_supported = is_chrome_browser && has_file_system_api;
                is_chrome.set(chrome_and_supported);

                if !chrome_and_supported {
                    show_chrome_warning.set(true);
                }
            }
        }
    });

    // Function to update cursor based on mouse position
    let mut update_cursor = move |mouse_x: f64, mouse_y: f64| {
        if is_crop_mode() {
            cursor_state.set("crosshair");
        } else if is_pointer_mode() {
            cursor_state.set("none"); // Hide cursor when pointer tool is active
            pointer_position.set((mouse_x, mouse_y));
        } else {
            let pos = position();
            let cam_size = size();

            // Check if mouse is within camera area (only if camera is enabled)
            if camera_enabled()
                && mouse_x >= pos.0
                && mouse_x <= pos.0 + cam_size.0
                && mouse_y >= pos.1
                && mouse_y <= pos.1 + cam_size.1
            {
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
        }
    };

    // Rendering loop for canvas
    let render_loop = move || {
        if let (Some(canvas), Some(camera_video)) = (canvas_ref(), camera_video_ref()) {
            if let Ok(Some(context)) = canvas.get_context("2d") {
                if let Ok(ctx) = context.dyn_into::<web_sys::CanvasRenderingContext2d>() {
                    let window = match web_sys::window() {
                        Some(w) => w,
                        None => return,
                    };
                    let viewport_width = window.inner_width().unwrap_or(1280.into()).as_f64().unwrap_or(1280.0);
                    let viewport_height = window.inner_height().unwrap_or(720.into()).as_f64().unwrap_or(720.0);

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
                                // Apply crop if set
                                let (src_x, src_y, src_width, src_height) =
                                    if let Some((cx, cy, cw, ch)) = crop_bounds() {
                                        (cx, cy, cw, ch)
                                    } else {
                                        (0.0, 0.0, video_width, video_height)
                                    };

                                // Calculate scale to fit canvas while maintaining aspect ratio (letterboxing)
                                let scale_x = viewport_width / src_width;
                                let scale_y = viewport_height / src_height;
                                let scale = scale_x.min(scale_y); // Use min to fit (letterbox if needed)

                                let scaled_width = src_width * scale;
                                let scaled_height = src_height * scale;

                                // Center the video on the canvas
                                let x = (viewport_width - scaled_width) / 2.0;
                                let y = (viewport_height - scaled_height) / 2.0;

                                let _ = ctx.draw_image_with_html_video_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                    &screen_video,
                                    src_x,
                                    src_y,
                                    src_width,
                                    src_height,
                                    x,
                                    y,
                                    scaled_width,
                                    scaled_height
                                );
                            }
                        }
                    }

                    // Draw camera overlay (only if camera is enabled)
                    if camera_enabled() && camera_video.ready_state() >= 2 {
                        let pos = position();
                        let cam_size = size();

                        // Save context state
                        ctx.save();

                        // Clip to rounded rectangle
                        ctx.begin_path();
                        ctx.move_to(pos.0 + 12.0, pos.1);
                        ctx.line_to(pos.0 + cam_size.0 - 12.0, pos.1);
                        ctx.quadratic_curve_to(
                            pos.0 + cam_size.0,
                            pos.1,
                            pos.0 + cam_size.0,
                            pos.1 + 12.0,
                        );
                        ctx.line_to(pos.0 + cam_size.0, pos.1 + cam_size.1 - 12.0);
                        ctx.quadratic_curve_to(
                            pos.0 + cam_size.0,
                            pos.1 + cam_size.1,
                            pos.0 + cam_size.0 - 12.0,
                            pos.1 + cam_size.1,
                        );
                        ctx.line_to(pos.0 + 12.0, pos.1 + cam_size.1);
                        ctx.quadratic_curve_to(
                            pos.0,
                            pos.1 + cam_size.1,
                            pos.0,
                            pos.1 + cam_size.1 - 12.0,
                        );
                        ctx.line_to(pos.0, pos.1 + 12.0);
                        ctx.quadratic_curve_to(pos.0, pos.1, pos.0 + 12.0, pos.1);
                        ctx.close_path();
                        ctx.clip();

                        // Flip horizontally for mirror effect
                        let _ = ctx.translate(pos.0 + cam_size.0 / 2.0, pos.1 + cam_size.1 / 2.0);
                        let _ = ctx.scale(-1.0, 1.0);
                        let _ =
                            ctx.translate(-(pos.0 + cam_size.0 / 2.0), -(pos.1 + cam_size.1 / 2.0));

                        // Draw camera video with zoom/crop and aspect ratio correction
                        let zoom = camera_zoom();
                        let video_width = camera_video.video_width() as f64;
                        let video_height = camera_video.video_height() as f64;
                        
                        if video_width > 0.0 && video_height > 0.0 {
                            // First apply zoom crop
                            let zoomed_width = video_width / zoom;
                            let zoomed_height = video_height / zoom;
                            
                            // Calculate aspect ratios
                            let video_aspect = zoomed_width / zoomed_height;
                            let camera_aspect = cam_size.0 / cam_size.1;
                            
                            // Calculate final crop dimensions to match camera aspect ratio
                            let (final_width, final_height) = if video_aspect > camera_aspect {
                                // Video is wider, crop horizontally
                                (zoomed_height * camera_aspect, zoomed_height)
                            } else {
                                // Video is taller, crop vertically
                                (zoomed_width, zoomed_width / camera_aspect)
                            };
                            
                            // Center the final crop area within the zoomed area
                            let base_x = (video_width - zoomed_width) / 2.0;
                            let base_y = (video_height - zoomed_height) / 2.0;
                            let crop_x = base_x + (zoomed_width - final_width) / 2.0;
                            let crop_y = base_y + (zoomed_height - final_height) / 2.0;
                            
                            // Draw the cropped video to fill the camera area exactly
                            let _ = ctx.draw_image_with_html_video_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                &camera_video,
                                crop_x,
                                crop_y,
                                final_width,
                                final_height,
                                pos.0,
                                pos.1,
                                cam_size.0,
                                cam_size.1,
                            );
                        }

                        // Restore context state
                        ctx.restore();
                    }

                    // Draw crop selection rectangle if in crop mode and drawing
                    if is_crop_mode() && is_drawing_crop() {
                        let start = crop_start();
                        let end = crop_end();

                        let x = (start.0 as f64).min(end.0 as f64);
                        let y = (start.1 as f64).min(end.1 as f64);
                        let width = (start.0 as f64 - end.0 as f64).abs();
                        let height = (start.1 as f64 - end.1 as f64).abs();

                        // Draw selection rectangle
                        ctx.set_stroke_style(&wasm_bindgen::JsValue::from_str(
                            "rgba(59, 130, 246, 0.8)",
                        ));
                        ctx.set_line_width(2.0);
                        ctx.stroke_rect(x, y, width, height);

                        // Draw semi-transparent overlay outside selection
                        ctx.set_fill_style(&wasm_bindgen::JsValue::from_str("rgba(0, 0, 0, 0.3)"));
                        // Top
                        ctx.fill_rect(0.0, 0.0, viewport_width, y);
                        // Bottom
                        ctx.fill_rect(
                            0.0,
                            y + height,
                            viewport_width,
                            viewport_height - (y + height),
                        );
                        // Left
                        ctx.fill_rect(0.0, y, x, height);
                        // Right
                        ctx.fill_rect(x + width, y, viewport_width - (x + width), height);
                    }

                    // Draw large pointer if in pointer mode
                    if is_pointer_mode() {
                        let pos = pointer_position();
                        let pointer_size = 60.0; // Large pointer

                        // Draw pointer arrow shape
                        ctx.set_fill_style(&wasm_bindgen::JsValue::from_str(
                            "rgba(255, 255, 255, 0.9)",
                        ));
                        ctx.set_stroke_style(&wasm_bindgen::JsValue::from_str(
                            "rgba(0, 0, 0, 0.8)",
                        ));
                        ctx.set_line_width(2.0);

                        ctx.begin_path();
                        // Arrow with acute angle pointing tip
                        ctx.move_to(pos.0, pos.1); // Tip of arrow
                        ctx.line_to(pos.0 + pointer_size * 0.2, pos.1 + pointer_size * 0.6); // Left side of shaft
                        ctx.line_to(pos.0 + pointer_size * 0.3, pos.1 + pointer_size * 0.3); // Indent for arrow notch
                        ctx.line_to(pos.0 + pointer_size * 0.6, pos.1 + pointer_size * 0.2); // Right side of shaft
                        ctx.close_path();

                        ctx.fill();
                        ctx.stroke();
                    }
                    
                    // Draw countdown if active
                    if countdown_value() > 0 {
                        ctx.save();
                        
                        // Set up extra large font for countdown
                        ctx.set_font("bold 400px Arial");
                        ctx.set_text_align("center");
                        ctx.set_text_baseline("middle");
                        
                        // White text with black outline
                        ctx.set_fill_style(&wasm_bindgen::JsValue::from_str("white"));
                        ctx.set_stroke_style(&wasm_bindgen::JsValue::from_str("black"));
                        ctx.set_line_width(12.0);
                        
                        let text = countdown_value().to_string();
                        let center_x = viewport_width / 2.0;
                        let center_y = viewport_height / 2.0;
                        
                        // Draw text with outline
                        let _ = ctx.stroke_text(&text, center_x, center_y);
                        let _ = ctx.fill_text(&text, center_x, center_y);
                        
                        ctx.restore();
                    }
                }
            }
        }
    };

    use_effect(move || {
        // Only start camera if it's enabled
        if camera_enabled() {
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
                                // Store the stream so we can stop it later
                                camera_stream.set(Some(media_stream.clone()));
                                
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
        }
    });

    // Start render loop with high-frequency setInterval (smoother than before)
    use_effect(move || {
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let render_loop_clone = render_loop.clone();

        let closure = Closure::wrap(Box::new(move || {
            render_loop_clone();
        }) as Box<dyn Fn()>);

        // 60fps = ~16.67ms, using 16ms for smoother rendering
        let id = window
            .set_interval_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                16,
            )
            .unwrap_or(-1);

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

                    if is_pointer_mode() {
                        // Always update pointer position when in pointer mode
                        cursor_state.set("none"); // Hide cursor when pointer tool is active
                        pointer_position.set((mouse_x, mouse_y));
                    } else if is_drawing_crop() {
                        crop_end.set((mouse_x, mouse_y));
                    } else if is_dragging() {
                        cursor_state.set("grabbing");
                        let offset = drag_offset();
                        let new_x = mouse_x - offset.0;
                        let new_y = mouse_y - offset.1;

                        // Get viewport dimensions
                        let window = match web_sys::window() {
                            Some(w) => w,
                            None => return,
                        };
                        let viewport_width = window.inner_width().unwrap_or(1280.into()).as_f64().unwrap_or(1280.0);
                        let viewport_height = window.inner_height().unwrap_or(720.into()).as_f64().unwrap_or(720.0);

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
                            new_width = (mouse_x - pos.0).max(100.0).min(600.0);
                        }
                        if corner.contains("bottom") {
                            new_height = (mouse_y - pos.1).max(100.0).min(400.0);
                        }

                        // Allow free aspect ratio changes - no constraint
                        size.set((new_width, new_height));
                    } else {
                        // Update cursor based on mouse position when not dragging/resizing
                        update_cursor(mouse_x, mouse_y);
                    }
                },
                onmousedown: move |event| {
                    let mouse_x = event.client_coordinates().x;
                    let mouse_y = event.client_coordinates().y;

                    if is_pointer_mode() {
                        // In pointer mode, just update pointer position
                        pointer_position.set((mouse_x, mouse_y));
                        event.stop_propagation();
                    } else if is_crop_mode() {
                        is_drawing_crop.set(true);
                        crop_start.set((mouse_x, mouse_y));
                        crop_end.set((mouse_x, mouse_y));
                        event.stop_propagation();
                    } else {
                        let pos = position();
                        let current_size = size();

                        // Check if click is within camera area (only if camera is enabled)
                        if camera_enabled() && mouse_x >= pos.0 && mouse_x <= pos.0 + current_size.0 &&
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
                    }
                },
                onmouseup: move |event| {
                    // Turn off pointer mode on any mouseup
                    if is_pointer_mode() {
                        is_pointer_mode.set(false);
                    }

                    if is_drawing_crop() {
                        is_drawing_crop.set(false);

                        // Calculate crop bounds in screen video coordinates
                        if let Some(screen_video) = screen_video_ref() {
                            if is_screen_sharing() && screen_video.ready_state() >= 2 {
                                let video_width = screen_video.video_width() as f64;
                                let video_height = screen_video.video_height() as f64;

                                if video_width > 0.0 && video_height > 0.0 {
                                    let window = match web_sys::window() {
                                        Some(w) => w,
                                        None => return,
                                    };
                                    let viewport_width = window.inner_width().unwrap_or(1280.into()).as_f64().unwrap_or(1280.0);
                                    let viewport_height = window.inner_height().unwrap_or(720.into()).as_f64().unwrap_or(720.0);

                                    // Calculate current video position on canvas
                                    let scale_x = viewport_width / video_width;
                                    let scale_y = viewport_height / video_height;
                                    let scale = scale_x.min(scale_y);

                                    let scaled_width = video_width * scale;
                                    let scaled_height = video_height * scale;
                                    let video_x = (viewport_width - scaled_width) / 2.0;
                                    let video_y = (viewport_height - scaled_height) / 2.0;

                                    // Convert crop selection to video coordinates
                                    let start = crop_start();
                                    let end = crop_end();

                                    let sel_x = start.0.min(end.0);
                                    let sel_y = start.1.min(end.1);
                                    let sel_width = (start.0 - end.0).abs();
                                    let sel_height = (start.1 - end.1).abs();

                                    // Convert to video coordinates
                                    let crop_x = ((sel_x - video_x) / scale).max(0.0).min(video_width);
                                    let crop_y = ((sel_y - video_y) / scale).max(0.0).min(video_height);
                                    let crop_w = (sel_width / scale).min(video_width - crop_x);
                                    let crop_h = (sel_height / scale).min(video_height - crop_y);

                                    if crop_w > 10.0 && crop_h > 10.0 {
                                        crop_bounds.set(Some((crop_x, crop_y, crop_w, crop_h)));
                                        is_crop_mode.set(false);
                                    }
                                }
                            }
                        }
                    }

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


            // Pointer tool button (top)
            button {
                style: format!("position: absolute; bottom: 426px; left: 20px; z-index: 10; width: 48px; height: 48px; background-color: {}; color: white; border: none; border-radius: 12px; cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 18px; transition: all 0.2s; box-shadow: 0 2px 8px rgba(0,0,0,0.3); font-family: monospace;",
                    if is_pointer_mode() { "#8b5cf6" } else { "#6366f1" }
                ),
                onmousedown: move |event| {
                    // Enable pointer mode and disable crop mode
                    is_pointer_mode.set(true);
                    is_crop_mode.set(false);
                    is_drawing_crop.set(false);

                    // Set initial pointer position to button center
                    let button_x = 44.0; // 20px left + 24px center of 48px button
                    let button_y = event.client_coordinates().y; // Use actual button y position
                    pointer_position.set((button_x, button_y));

                    event.stop_propagation();
                },
                // Pointer icon
                "👆"
            },

            // Record button (2nd from top)
            button {
                style: format!("position: absolute; bottom: 368px; left: 20px; z-index: 10; width: 48px; height: 48px; background-color: {}; color: white; border: none; border-radius: 12px; cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 18px; transition: all 0.2s; box-shadow: 0 2px 8px rgba(0,0,0,0.3); font-family: monospace;",
                    if is_recording() { "#ef4444" } else { "#dc2626" }
                ),
                onclick: move |_| {
                    if is_recording() {
                        // Stop recording
                        if let Some(recorder) = media_recorder() {
                            recorder.stop().unwrap_or(());
                        }
                        is_recording.set(false);

                        // Close file stream if using Chrome File System Access API
                        if is_chrome() {
                            if let Some(stream) = file_stream() {
                                spawn(async move {
                                    // Use js_sys to call close method
                                    if let Ok(close_method) = js_sys::Reflect::get(&stream, &"close".into()) {
                                        if let Ok(func) = close_method.dyn_into::<js_sys::Function>() {
                                            if let Ok(promise_js) = js_sys::Reflect::apply(
                                                &func,
                                                &stream,
                                                &js_sys::Array::new()
                                            ) {
                                                if let Ok(promise) = promise_js.dyn_into::<js_sys::Promise>() {
                                                    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                                                }
                                            }
                                        }
                                    }
                                });
                                file_stream.set(None);
                                file_handle.set(None);
                            }
                        }
                    } else {
                        // Start countdown before recording
                        countdown_value.set(3);
                        
                        // Start countdown timer
                        let mut countdown_clone = countdown_value.clone();
                        let mut is_recording_clone = is_recording.clone();
                        let mut recorded_chunks_clone = recorded_chunks.clone();
                        let mut media_recorder_clone = media_recorder.clone();
                        let canvas_ref_clone = canvas_ref.clone();
                        
                        spawn(async move {
                            // Countdown from 3 to 1
                            for i in (1..=3).rev() {
                                countdown_clone.set(i);
                                gloo_timers::future::TimeoutFuture::new(1000).await;
                            }
                            
                            // Clear countdown and start recording
                            countdown_clone.set(0);
                            
                            // Start recording logic (moved from original)
                            if let Some(canvas) = canvas_ref_clone() {
                                if let Ok(canvas_stream) = canvas.capture_stream() {
                                    // Get microphone audio
                                    if let Some(window) = web_sys::window() {
                                        let navigator = window.navigator();
                                        if let Ok(media_devices) = navigator.media_devices() {
                                            let audio_constraints = web_sys::MediaStreamConstraints::new();
                                            audio_constraints.set_audio(&true.into());
                                            audio_constraints.set_video(&false.into());

                                            if let Ok(promise) = media_devices.get_user_media_with_constraints(&audio_constraints) {
                                                let future = wasm_bindgen_futures::JsFuture::from(promise);
                                                if let Ok(stream) = future.await {
                                                    if let Ok(audio_stream) = stream.dyn_into::<web_sys::MediaStream>() {
                                                        // Combine canvas and audio streams
                                                        let audio_tracks = audio_stream.get_audio_tracks();
                                                        for i in 0..audio_tracks.length() {
                                                            let track = audio_tracks.get(i);
                                                            if let Ok(audio_track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                                                                canvas_stream.add_track(&audio_track);
                                                            }
                                                        }

                                                        // Create MediaRecorder
                                                        if let Ok(recorder) = web_sys::MediaRecorder::new_with_media_stream(&canvas_stream) {
                                                            // Clear previous recordings
                                                            recorded_chunks_clone.set(Vec::new());

                                                            // Set up data available handler
                                                            let mut chunks_clone2 = recorded_chunks_clone.clone();
                                                            let data_handler = Closure::wrap(Box::new(move |event: web_sys::BlobEvent| {
                                                                if let Some(data) = event.data() {
                                                                    let mut current_chunks = chunks_clone2();
                                                                    current_chunks.push(data);
                                                                    chunks_clone2.set(current_chunks);
                                                                }
                                                            }) as Box<dyn FnMut(web_sys::BlobEvent)>);

                                                            recorder.set_ondataavailable(Some(data_handler.as_ref().unchecked_ref()));
                                                            data_handler.forget();

                                                            // Set up stop handler  
                                                            let recorded_chunks_clone3 = recorded_chunks_clone.clone();
                                                            let stop_handler = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                                                                // Create and download blob when recording stops
                                                                let chunks = recorded_chunks_clone3();
                                                                if !chunks.is_empty() {
                                                                    let blob_parts = js_sys::Array::new();
                                                                    for chunk in chunks {
                                                                        blob_parts.push(&chunk);
                                                                    }

                                                                    let blob_options = web_sys::BlobPropertyBag::new();
                                                                    blob_options.set_type("video/webm");

                                                                    if let Ok(blob) = web_sys::Blob::new_with_blob_sequence_and_options(&blob_parts, &blob_options) {
                                                                        // Create download link
                                                                        if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                                                                            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                                                                                if let Ok(link) = document.create_element("a") {
                                                                                    if let Ok(anchor) = link.dyn_into::<web_sys::HtmlAnchorElement>() {
                                                                                        anchor.set_href(&url);
                                                                                        // Generate timestamp-based filename
                                                                                        let now = js_sys::Date::new_0();
                                                                                        let year = now.get_full_year() as i32;
                                                                                        let month = (now.get_month() as f64 + 1.0) as i32; // getMonth() returns 0-11, so add 1
                                                                                        let day = now.get_date() as i32;
                                                                                        let hours = now.get_hours() as i32;
                                                                                        let minutes = now.get_minutes() as i32;
                                                                                        
                                                                                        let filename = format!(
                                                                                            "demo {}-{:02}-{:02} {:02}:{:02}.webm",
                                                                                            year, month, day, hours, minutes
                                                                                        );
                                                                                        anchor.set_download(&filename);
                                                                                        anchor.click();
                                                                                        let _ = web_sys::Url::revoke_object_url(&url);
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }) as Box<dyn Fn(web_sys::Event)>);

                                                            recorder.set_onstop(Some(stop_handler.as_ref().unchecked_ref()));
                                                            stop_handler.forget();

                                                            // Start recording
                                                            recorder.start().unwrap_or(());
                                                            media_recorder_clone.set(Some(recorder));
                                                            is_recording_clone.set(true);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                },
                // Record icon
                {if is_recording() { "⏹" } else { "⏺" }}
            },

            // Picture-in-Picture button (3rd from top)
            button {
                style: format!("position: absolute; bottom: 310px; left: 20px; z-index: 10; width: 48px; height: 48px; background-color: {}; color: white; border: none; border-radius: 12px; cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 18px; transition: all 0.2s; box-shadow: 0 2px 8px rgba(0,0,0,0.3); font-family: monospace;",
                    if is_pip_active() { "#10b981" } else { "#6366f1" }
                ),
                onclick: move |_| {
                    web_sys::console::log_1(&"PiP button clicked".into());
                    if is_pip_active() {
                        web_sys::console::log_1(&"Exiting PiP".into());
                        // Exit PiP
                        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                            if let Ok(promise) = js_sys::Reflect::get(&document, &"exitPictureInPicture".into()) {
                                if let Ok(func) = promise.dyn_into::<js_sys::Function>() {
                                    let mut is_pip_clone = is_pip_active.clone();
                                    spawn(async move {
                                        if let Ok(promise) = func.call0(&document) {
                                            if let Ok(promise) = promise.dyn_into::<js_sys::Promise>() {
                                                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                                            }
                                        }
                                        is_pip_clone.set(false);
                                    });
                                }
                            }
                        }
                    } else {
                        web_sys::console::log_1(&"Entering PiP".into());
                        // Enter PiP
                        if let Some(canvas) = canvas_ref() {
                            web_sys::console::log_1(&"Got canvas".into());
                            let mut pip_video_clone = pip_video_element.clone();
                            let mut is_pip_clone = is_pip_active.clone();
                            spawn(async move {
                                web_sys::console::log_1(&"In spawn".into());
                                if let Ok(stream) = canvas.capture_stream() {
                                    web_sys::console::log_1(&"Got stream".into());
                                    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                                        web_sys::console::log_1(&"Got document".into());
                                        // Create or reuse video element
                                        let video = if let Some(existing_video) = pip_video_clone() {
                                            web_sys::console::log_1(&"Using existing video".into());
                                            existing_video
                                        } else {
                                            web_sys::console::log_1(&"Creating new video".into());
                                            if let Ok(video_elem) = document.create_element("video") {
                                                if let Ok(video) = video_elem.dyn_into::<web_sys::HtmlVideoElement>() {
                                                    // Hide the video element
                                                    if let Some(html_elem) = video.dyn_ref::<web_sys::HtmlElement>() {
                                                        html_elem.style().set_property("display", "none").ok();
                                                    }
                                                    if let Some(body) = document.body() {
                                                        body.append_child(&video).ok();
                                                    }
                                                    pip_video_clone.set(Some(video.clone()));
                                                    video
                                                } else {
                                                    web_sys::console::log_1(&"Failed to cast to video".into());
                                                    return;
                                                }
                                            } else {
                                                web_sys::console::log_1(&"Failed to create video element".into());
                                                return;
                                            }
                                        };
                                        
                                        web_sys::console::log_1(&"Setting up video".into());
                                        video.set_src_object(Some(&stream));
                                        video.set_muted(true);
                                        let _ = video.play();
                                        
                                        // Wait for video metadata to load before requesting PiP
                                        let video_clone = video.clone();
                                        let mut is_pip_clone2 = is_pip_clone.clone();
                                        let is_recording_clone = is_recording.clone();
                                        let media_recorder_clone = media_recorder.clone();
                                        let is_chrome_clone = is_chrome.clone();
                                        let file_stream_clone = file_stream.clone();
                                        let file_handle_clone = file_handle.clone();
                                        let callback = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                                            web_sys::console::log_1(&"Video metadata loaded, requesting PiP".into());
                                            if let Ok(promise) = js_sys::Reflect::get(&video_clone, &"requestPictureInPicture".into()) {
                                                web_sys::console::log_1(&"Got PiP method".into());
                                                if let Ok(func) = promise.dyn_into::<js_sys::Function>() {
                                                    web_sys::console::log_1(&"Calling PiP".into());
                                                    if let Ok(_promise) = func.call0(&video_clone) {
                                                        web_sys::console::log_1(&"PiP call succeeded".into());
                                                        is_pip_clone2.set(true);
                                                        
                                                        // Add event listener for when PiP window is closed
                                                        let video_clone2 = video_clone.clone();
                                                        let mut is_recording_clone2 = is_recording_clone.clone();
                                                        let media_recorder_clone2 = media_recorder_clone.clone(); 
                                                        let mut is_pip_clone3 = is_pip_clone2.clone();
                                                        let is_chrome_clone2 = is_chrome_clone.clone();
                                                        let mut file_stream_clone2 = file_stream_clone.clone();
                                                        let mut file_handle_clone2 = file_handle_clone.clone();
                                                        
                                                        let leave_pip_callback = wasm_bindgen::closure::Closure::wrap(Box::new(move |_event: web_sys::Event| {
                                                            web_sys::console::log_1(&"PiP window closed, stopping recording".into());
                                                            is_pip_clone3.set(false);
                                                            
                                                            // Stop recording if it's active
                                                            if is_recording_clone2() {
                                                                if let Some(recorder) = media_recorder_clone2() {
                                                                    recorder.stop().unwrap_or(());
                                                                }
                                                                is_recording_clone2.set(false);
                                                                
                                                                // Close file stream if using Chrome File System Access API
                                                                if is_chrome_clone2() {
                                                                    if let Some(stream) = file_stream_clone2() {
                                                                        spawn(async move {
                                                                            // Use js_sys to call close method
                                                                            if let Ok(close_method) = js_sys::Reflect::get(&stream, &"close".into()) {
                                                                                if let Ok(func) = close_method.dyn_into::<js_sys::Function>() {
                                                                                    if let Ok(promise_js) = js_sys::Reflect::apply(
                                                                                        &func,
                                                                                        &stream,
                                                                                        &js_sys::Array::new()
                                                                                    ) {
                                                                                        if let Ok(promise) = promise_js.dyn_into::<js_sys::Promise>() {
                                                                                            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                                                                                        }
                                                                                    }
                                                                                }
                                                                            }
                                                                        });
                                                                        file_stream_clone2.set(None);
                                                                        file_handle_clone2.set(None);
                                                                    }
                                                                }
                                                            }
                                                        }) as Box<dyn FnMut(web_sys::Event)>);
                                                        
                                                        video_clone2.add_event_listener_with_callback("leavepictureinpicture", leave_pip_callback.as_ref().unchecked_ref()).ok();
                                                        leave_pip_callback.forget();
                                                        
                                                    } else {
                                                        web_sys::console::log_1(&"Failed to call PiP".into());
                                                    }
                                                } else {
                                                    web_sys::console::log_1(&"Failed to cast to function".into());
                                                }
                                            } else {
                                                web_sys::console::log_1(&"No PiP method found".into());
                                            }
                                        }) as Box<dyn FnMut()>);
                                        
                                        video.add_event_listener_with_callback("loadedmetadata", callback.as_ref().unchecked_ref()).ok();
                                        callback.forget(); // Keep callback alive
                                    }
                                } else {
                                    web_sys::console::log_1(&"Failed to get stream".into());
                                }
                            });
                        } else {
                            web_sys::console::log_1(&"No canvas found".into());
                        }
                    }
                },
                // PiP icon
                "🖼"
            },

            // Reset zoom button (4th from top)
            button {
                style: format!("position: absolute; bottom: 252px; left: 20px; z-index: 10; width: 48px; height: 48px; background-color: {}; color: white; border: none; border-radius: 12px; cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 18px; transition: all 0.2s; box-shadow: 0 2px 8px rgba(0,0,0,0.3); font-family: monospace;",
                    if crop_bounds().is_some() { "#f59e0b" } else { "#6b7280" }
                ),
                onclick: move |_| {
                    crop_bounds.set(None);
                    is_crop_mode.set(false);
                },
                disabled: crop_bounds().is_none(),
                // Reset/full screen icon
                "⤢"
            },

            // Crop/zoom button (5th from top)
            button {
                style: format!("position: absolute; bottom: 194px; left: 20px; z-index: 10; width: 48px; height: 48px; background-color: {}; color: white; border: none; border-radius: 12px; cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 18px; transition: all 0.2s; box-shadow: 0 2px 8px rgba(0,0,0,0.3); font-family: monospace;",
                    if is_crop_mode() { "#8b5cf6" } else if crop_bounds().is_some() { "#a78bfa" } else { "#6366f1" }
                ),
                onclick: move |_| {
                    is_crop_mode.set(!is_crop_mode());
                    is_drawing_crop.set(false);
                },
                disabled: !is_screen_sharing(),
                // Crop icon
                "⬚"
            },

            // Screen share button (6th from top)
            button {
                style: format!("position: absolute; bottom: 136px; left: 20px; z-index: 10; width: 48px; height: 48px; background-color: {}; color: white; border: none; border-radius: 12px; cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 18px; transition: all 0.2s; box-shadow: 0 2px 8px rgba(0,0,0,0.3); font-family: monospace;",
                    if is_screen_sharing() { "#ef4444" } else { "#3b82f6" }
                ),
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
                                                
                                                // Reset screen share crop when starting
                                                crop_bounds.set(None);
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                },
                // Screen share specific icons  
                {if is_screen_sharing() { "📴" } else { "🖥" }}
            },

            // Camera toggle button (above camera zoom)
            button {
                style: format!("position: absolute; bottom: 78px; left: 20px; z-index: 10; width: 48px; height: 48px; background-color: {}; color: white; border: none; border-radius: 12px; cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 18px; transition: all 0.2s; box-shadow: 0 2px 8px rgba(0,0,0,0.3); font-family: monospace;", 
                    if camera_enabled() { "#10b981" } else { "#6b7280" }
                ),
                onclick: move |_| {
                    if camera_enabled() {
                        // Stop camera stream
                        if let Some(stream) = camera_stream() {
                            let tracks = stream.get_tracks();
                            for i in 0..tracks.length() {
                                let track = tracks.get(i);
                                if let Ok(media_track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                                    media_track.stop();
                                }
                            }
                        }
                        camera_stream.set(None);
                        
                        // Clear video element
                        if let Some(video_elem) = camera_video_ref() {
                            video_elem.set_src_object(None);
                        }
                        
                        camera_enabled.set(false);
                    } else {
                        // Start camera stream
                        spawn(async move {
                            if let Some(window) = web_sys::window() {
                                let navigator = window.navigator();
                                if let Ok(media_devices) = navigator.media_devices() {
                                    let constraints = web_sys::MediaStreamConstraints::new();
                                    constraints.set_video(&true.into());
                                    constraints.set_audio(&false.into());

                                    if let Ok(promise) = media_devices.get_user_media_with_constraints(&constraints) {
                                        let future = wasm_bindgen_futures::JsFuture::from(promise);
                                        if let Ok(stream) = future.await {
                                            if let Ok(media_stream) = stream.dyn_into::<web_sys::MediaStream>() {
                                                camera_stream.set(Some(media_stream.clone()));
                                                
                                                if let Some(video_elem) = camera_video_ref() {
                                                    video_elem.set_src_object(Some(&media_stream));
                                                    let _ = video_elem.play();
                                                }
                                                
                                                camera_enabled.set(true);
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                },
                // Camera icon
                {if camera_enabled() { "📹" } else { "📴" }}
            },

            // Camera zoom slider (bottom)
            div {
                style: "position: absolute; bottom: 20px; left: 20px; z-index: 10; width: 160px; height: 48px; background-color: rgba(31, 41, 55, 0.9); padding: 6px 8px; border-radius: 12px; box-shadow: 0 2px 8px rgba(0,0,0,0.3); display: flex; flex-direction: column; justify-content: center;",
                div {
                    style: "color: white; font-size: 10px; margin-bottom: 2px; font-family: monospace; text-align: center;",
                    {format!("Zoom: {:.1}x", camera_zoom())}
                }
                input {
                    r#type: "range",
                    min: "1.0",
                    max: "3.0",
                    step: "0.1",
                    value: format!("{}", camera_zoom()),
                    style: "width: 100%; accent-color: #6366f1; height: 16px;",
                    oninput: move |event| {
                        if let Ok(value) = event.value().parse::<f64>() {
                            camera_zoom.set(value);
                        }
                    }
                }
            }

            // Chrome Warning Modal
            if show_chrome_warning() {
                div {
                    style: "position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; background-color: rgba(0, 0, 0, 0.8); z-index: 1000; display: flex; align-items: center; justify-content: center;",
                    div {
                        style: "background-color: #1f2937; color: white; padding: 32px; border-radius: 16px; max-width: 500px; text-align: center; box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.5);",
                        h2 {
                            style: "margin: 0 0 16px 0; color: #f59e0b; font-size: 24px;",
                            "⚠️ Chrome Required"
                        }
                        p {
                            style: "margin: 0 0 24px 0; line-height: 1.6; color: #d1d5db;",
                            "This application is designed specifically for Google Chrome and uses advanced Chrome-only features like the File System Access API for optimal recording experience."
                        }
                        p {
                            style: "margin: 0 0 24px 0; line-height: 1.6; color: #d1d5db;",
                            "For the best experience with unlimited recording length and direct file saving, please use Chrome."
                        }
                        div {
                            style: "display: flex; gap: 12px; justify-content: center;",
                            button {
                                style: "background-color: #3b82f6; color: white; border: none; padding: 12px 24px; border-radius: 8px; cursor: pointer; font-weight: 600; transition: background-color 0.2s;",
                                onclick: move |_| {
                                    if let Some(window) = web_sys::window() {
                                        let _ = window.open_with_url_and_target("https://www.google.com/chrome/", "_blank");
                                    }
                                },
                                "Get Chrome"
                            }
                            button {
                                style: "background-color: #6b7280; color: white; border: none; padding: 12px 24px; border-radius: 8px; cursor: pointer; font-weight: 600; transition: background-color 0.2s;",
                                onclick: move |_| {
                                    show_chrome_warning.set(false);
                                },
                                "Continue Anyway"
                            }
                        }
                    }
                }
            }
        }
    }
}
