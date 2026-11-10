//! Browser backend (wasm32) for the internal window layer.
//!
//! The framebuffer is copied to a `<canvas>` with `putImageData` once per
//! animation frame. DOM mouse, keyboard and wheel events are translated into
//! the same [`Event`] model the native backends use. Virtual-key codes match
//! the Windows and X11 backends, so `Scene` sees one set of keys.
//!
//! [`start`] returns as soon as the frame loop is scheduled. The loop keeps
//! running for the life of the page.
//!
//! Type-checked for `wasm32-unknown-unknown` only. It has not been run in a
//! browser from this repository.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{Clamped, JsCast, JsValue};
use web_sys::{
    CanvasRenderingContext2d, HtmlCanvasElement, HtmlElement, ImageData, KeyboardEvent, MouseEvent,
    WheelEvent,
};

use super::{pixel_count, pixels_to_rgba, ControlFlow, Event, Frame, MouseButton};

/// Background colour used to clear the framebuffer before each frame.
const CLEAR: u32 = 0x00FF_FFFF;

/// Starts the frame loop on the `<canvas id="...">` element `canvas_id`.
///
/// `on_frame` runs once per animation frame with the canvas's current size.
/// Returning [`ControlFlow::Exit`] stops the loop.
pub(crate) fn start(
    canvas_id: &str,
    on_frame: impl FnMut(Frame<'_>) -> ControlFlow + 'static,
) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let element = document
        .get_element_by_id(canvas_id)
        .ok_or_else(|| JsValue::from_str(&format!("no element with id `{canvas_id}`")))?;
    let canvas: HtmlCanvasElement = element
        .dyn_into()
        .map_err(|_| JsValue::from_str(&format!("element `{canvas_id}` is not a <canvas>")))?;
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("2d context unavailable"))?
        .dyn_into()?;

    // A canvas only receives key events when it is focusable.
    canvas.set_attribute("tabindex", "0")?;

    let queue: Rc<RefCell<Vec<Event>>> = Rc::new(RefCell::new(Vec::new()));
    attach_pointer(&canvas, queue.clone())?;
    attach_keyboard(&canvas, queue.clone())?;
    attach_wheel(&canvas, queue.clone())?;

    let mut on_frame = on_frame;
    let mut pixels: Vec<u32> = Vec::new();
    let mut last_size = (0u32, 0u32);

    // The loop closure reschedules itself, so it lives in a cell that it
    // captures. The page owns it for good, so the cycle is not reclaimed.
    type FrameCell = Rc<RefCell<Option<Closure<dyn FnMut()>>>>;
    let frame: FrameCell = Rc::new(RefCell::new(None));
    let frame_for_tick = frame.clone();
    let tick_canvas = canvas.clone();
    let tick = Closure::wrap(Box::new(move || {
        let mut events: Vec<Event> = std::mem::take(&mut *queue.borrow_mut());
        let (w, h) = (tick_canvas.width(), tick_canvas.height());
        if (w, h) != last_size {
            last_size = (w, h);
            events.insert(
                0,
                Event::Resized {
                    width: w,
                    height: h,
                },
            );
        }

        if w > 0 && h > 0 {
            if pixels.len() != pixel_count(w, h) {
                pixels.clear();
                pixels.resize(pixel_count(w, h), CLEAR);
            }
            pixels.fill(CLEAR);

            let flow = on_frame(Frame {
                width: w,
                height: h,
                pixels: &mut pixels,
                events: &events,
            });

            let rgba = pixels_to_rgba(&pixels);
            if let Ok(image) = ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), w, h) {
                let _ = ctx.put_image_data(&image, 0.0, 0.0);
            }
            if flow == ControlFlow::Exit {
                return;
            }
        }

        if let Some(next) = frame_for_tick.borrow().as_ref() {
            let _ = request_frame(next);
        }
    }) as Box<dyn FnMut()>);

    *frame.borrow_mut() = Some(tick);
    if let Some(first) = frame.borrow().as_ref() {
        request_frame(first)?;
    }
    Ok(())
}

fn request_frame(callback: &Closure<dyn FnMut()>) -> Result<i32, JsValue> {
    web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .request_animation_frame(callback.as_ref().unchecked_ref())
}

/// Maps a client (viewport) position to canvas pixels, which differ from CSS
/// pixels when the canvas is scaled.
fn canvas_point(canvas: &HtmlCanvasElement, client_x: i32, client_y: i32) -> (i32, i32) {
    let rect = canvas.get_bounding_client_rect();
    let scale_x = if rect.width() > 0.0 {
        f64::from(canvas.width()) / rect.width()
    } else {
        1.0
    };
    let scale_y = if rect.height() > 0.0 {
        f64::from(canvas.height()) / rect.height()
    } else {
        1.0
    };
    (
        ((f64::from(client_x) - rect.left()) * scale_x).round() as i32,
        ((f64::from(client_y) - rect.top()) * scale_y).round() as i32,
    )
}

fn button_of(button: i16) -> MouseButton {
    match button {
        0 => MouseButton::Primary,
        1 => MouseButton::Middle,
        2 => MouseButton::Secondary,
        n => MouseButton::Other(n.max(0) as u8),
    }
}

/// Registers a DOM listener that pushes events onto `queue`. The closure is
/// leaked on purpose: the canvas lives as long as the page.
fn listen<E: wasm_bindgen::convert::FromWasmAbi + 'static>(
    canvas: &HtmlCanvasElement,
    name: &str,
    handler: impl FnMut(E) + 'static,
) -> Result<(), JsValue> {
    let closure = Closure::wrap(Box::new(handler) as Box<dyn FnMut(E)>);
    canvas.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())?;
    closure.forget();
    Ok(())
}

fn attach_pointer(
    canvas: &HtmlCanvasElement,
    queue: Rc<RefCell<Vec<Event>>>,
) -> Result<(), JsValue> {
    let (q_down, q_up, q_move) = (queue.clone(), queue.clone(), queue);
    let (c_down, c_up, c_move) = (canvas.clone(), canvas.clone(), canvas.clone());

    listen::<MouseEvent>(canvas, "mousedown", move |e| {
        // Take keyboard focus so key events reach this canvas.
        let _ = c_down.unchecked_ref::<HtmlElement>().focus();
        let (x, y) = canvas_point(&c_down, e.client_x(), e.client_y());
        q_down.borrow_mut().push(Event::PointerDown {
            x,
            y,
            button: button_of(e.button()),
        });
    })?;
    listen::<MouseEvent>(canvas, "mouseup", move |e| {
        let (x, y) = canvas_point(&c_up, e.client_x(), e.client_y());
        q_up.borrow_mut().push(Event::PointerUp {
            x,
            y,
            button: button_of(e.button()),
        });
    })?;
    listen::<MouseEvent>(canvas, "mousemove", move |e| {
        let (x, y) = canvas_point(&c_move, e.client_x(), e.client_y());
        q_move.borrow_mut().push(Event::PointerMoved { x, y });
    })?;
    Ok(())
}

fn attach_keyboard(
    canvas: &HtmlCanvasElement,
    queue: Rc<RefCell<Vec<Event>>>,
) -> Result<(), JsValue> {
    listen::<KeyboardEvent>(canvas, "keydown", move |e| {
        let key = e.key();
        let (shift, ctrl, alt, meta) = (e.shift_key(), e.ctrl_key(), e.alt_key(), e.meta_key());
        if let Some(code) = dom_key_to_vk(&key) {
            // Keep Tab, Enter, arrows and the like from scrolling or leaving the canvas.
            e.prevent_default();
            queue.borrow_mut().push(Event::KeyDown {
                code,
                shift,
                ctrl,
                alt,
            });
        } else if key.chars().count() == 1 && !ctrl && !alt && !meta {
            if let Some(ch) = key.chars().next() {
                queue.borrow_mut().push(Event::Text(ch));
            }
        }
    })
}

fn attach_wheel(canvas: &HtmlCanvasElement, queue: Rc<RefCell<Vec<Event>>>) -> Result<(), JsValue> {
    let c = canvas.clone();
    listen::<WheelEvent>(canvas, "wheel", move |e| {
        e.prevent_default();
        let (x, y) = canvas_point(&c, e.client_x(), e.client_y());
        // DOM deltaY is positive when scrolling down; the crate's delta is
        // positive away from the user. DOM_DELTA_PIXEL is mode 0, otherwise lines.
        let raw = e.delta_y();
        let delta = if e.delta_mode() == 0 {
            -raw / 100.0
        } else {
            -raw
        };
        queue.borrow_mut().push(Event::Wheel {
            x,
            y,
            delta: delta as f32,
        });
    })
}

/// The virtual-key code for a DOM `KeyboardEvent.key` name, for keys that do
/// not type a character.
fn dom_key_to_vk(key: &str) -> Option<u32> {
    Some(match key {
        "Backspace" => 0x08,
        "Tab" => 0x09,
        "Enter" => 0x0D,
        "Escape" => 0x1B,
        "End" => 0x23,
        "Home" => 0x24,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "Delete" => 0x2E,
        _ => return None,
    })
}
