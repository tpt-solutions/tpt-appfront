//! Smoke test for the internal window layer and painter: opens a window,
//! draws a bordered panel with text, a square that moves each frame, and logs
//! input events. It exits after 240 frames (or 1500 with `-- 1500`), or when
//! the close button is pressed.
//!
//! Run: `cargo run -p tpt-appfront-canvas --example platform_smoke`

use tpt_appfront_canvas::glyphs::Font;
use tpt_appfront_canvas::platform::{self, ControlFlow, Event};
use tpt_appfront_canvas::raster::{rgb, Canvas, Rect};

fn main() {
    // Optional frame count: `platform_smoke 1500` keeps the window up longer.
    let frames: u32 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(240);
    let mut font = Font::bundled();
    let mut frame_no = 0u32;

    let result = platform::run("platform smoke", 480, 320, |frame| {
        for event in frame.events {
            match event {
                Event::CloseRequested => {
                    println!("close requested");
                    return ControlFlow::Exit;
                }
                other => println!("{other:?}"),
            }
        }

        let (w, h) = (frame.width as usize, frame.height as usize);
        let mut canvas = Canvas::new(&mut *frame.pixels, w, h);

        // A bordered panel with a heading, drawn with the painter.
        canvas.fill_rect(Rect::new(16, 16, 448, 60), rgb(0xF3, 0xF4, 0xF6));
        canvas.stroke_rect(Rect::new(16, 16, 448, 60), rgb(0x3B, 0x82, 0xF6), 2);
        canvas.draw_text(
            &mut font,
            32.0,
            52.0,
            "Hello from the internal painter",
            24.0,
            rgb(0x37, 0x41, 0x51),
        );

        // Escape-hatch painting: a moving square written straight into the buffer.
        let x0 = (frame_no as usize * 2) % w.max(1);
        let y0 = h / 2 + 30;
        canvas.fill_rect(
            Rect::new(x0 as i32, y0 as i32, 40, 40),
            rgb(0x00, 0x33, 0xCC),
        );

        frame_no += 1;
        if frame_no >= frames {
            ControlFlow::Exit
        } else {
            ControlFlow::Continue
        }
    });

    match result {
        Ok(()) => println!("ok after {frame_no} frames"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
