//! X11 backend for the internal window layer (see `platform/mod.rs`).
//!
//! Speaks the X11 wire protocol directly, using `x11rb-protocol` for request
//! encoding, reply parsing and the connection handshake. That crate is pure
//! Rust and needs neither libX11 nor libxcb at build or run time. The full
//! `x11rb` crate is avoided because it depends on `gethostname`, which is on
//! the ban list in `deny.toml`.
//!
//! Pixels are presented with `PutImage` (ZPixmap, 32 bits per pixel, depth 24).
//! The framebuffer's `0x00RRGGBB` values match an LSB-first TrueColor visual,
//! which is what X servers provide by default. Other layouts are reported as
//! errors rather than drawn wrongly.
//!
//! Type-checked for `x86_64-unknown-linux-gnu`. It has not been run against a
//! live X server from this repository: no X server was available when it was
//! written.

use std::borrow::Cow;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use x11rb_protocol::connect::Connect;
use x11rb_protocol::parse_display::{parse_display, ConnectAddress};
use x11rb_protocol::protocol::xproto::{
    ChangePropertyRequest, CreateGCAux, CreateGCRequest, CreateWindowAux, CreateWindowRequest,
    EventMask, GetInputFocusRequest, GetKeyboardMappingReply, GetKeyboardMappingRequest,
    ImageFormat, ImageOrder, InternAtomReply, InternAtomRequest, MapWindowRequest, PropMode,
    PutImageRequest, Setup, WindowClass,
};
use x11rb_protocol::x11_utils::TryParse;
use x11rb_protocol::xauth::Family;

use super::{pixel_count, ControlFlow, Event, Frame, MouseButton, PlatformError};

/// Background colour used to clear the framebuffer before each frame.
const CLEAR: u32 = 0x00FF_FFFF;
/// Every event, error, and reply header is at least this long.
const PACKET: usize = 32;
/// Bytes kept free in each PutImage request for its header.
const PUT_IMAGE_HEADER: usize = 64;
/// The predefined X atom type `ATOM`, used as the type of `WM_PROTOCOLS`.
const ATOM_TYPE: u32 = 4;

// X modifier bits in the `state` field of key and button events.
const SHIFT_MASK: u16 = 1 << 0;
const CONTROL_MASK: u16 = 1 << 2;
const MOD1_MASK: u16 = 1 << 3;

/// Keysyms for keys reported as virtual-key codes. The codes match the Windows
/// backend, so the rest of the crate sees one set of keys.
const KEYSYM_TABLE: &[(u32, u32)] = &[
    (0xff08, 0x08), // BackSpace
    (0xff09, 0x09), // Tab
    (0xff0d, 0x0D), // Return
    (0xff8d, 0x0D), // KP_Enter
    (0xff1b, 0x1B), // Escape
    (0xff50, 0x24), // Home
    (0xff51, 0x25), // Left
    (0xff52, 0x26), // Up
    (0xff53, 0x27), // Right
    (0xff54, 0x28), // Down
    (0xff57, 0x23), // End
    (0xffff, 0x2E), // Delete
];

/// A connection to the X server: a Unix socket or TCP.
enum Stream {
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Unix(s) => s.read(buf),
            Stream::Tcp(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Unix(s) => s.write(buf),
            Stream::Tcp(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Unix(s) => s.flush(),
            Stream::Tcp(s) => s.flush(),
        }
    }
}

impl Stream {
    fn set_nonblocking(&self, on: bool) -> io::Result<()> {
        match self {
            Stream::Unix(s) => s.set_nonblocking(on),
            Stream::Tcp(s) => s.set_nonblocking(on),
        }
    }
}

struct Connection {
    stream: Stream,
    setup: Setup,
    /// Sequence number of the last request sent. The first request is 1.
    seq: u16,
    /// Bytes received but not yet split into packets.
    inbuf: Vec<u8>,
    /// Error packets seen while waiting for something else, by sequence number.
    errors: Vec<(u16, String)>,
    min_keycode: u8,
    keysyms_per_keycode: usize,
    keysyms: Vec<u32>,
    wm_delete: u32,
}

pub(super) fn run(
    title: &str,
    width: u32,
    height: u32,
    mut on_frame: impl FnMut(Frame<'_>) -> ControlFlow,
) -> Result<(), PlatformError> {
    let mut conn = Connection::open()?;
    let root_depth = conn.setup.roots.first().map_or(0, |s| s.root_depth);
    if root_depth != 24 {
        return Err(PlatformError::Os(format!(
            "X11 root window depth is {root_depth}; only 24-bit TrueColor is supported"
        )));
    }
    if u8::from(conn.setup.image_byte_order) != u8::from(ImageOrder::LSB_FIRST) {
        return Err(PlatformError::Os(
            "X11 server sends big-endian images; only LSB-first is supported".into(),
        ));
    }

    let window = conn.create_window(title, width, height)?;
    let gc = conn.setup.resource_id_base + 1;
    conn.send_checked(
        CreateGCRequest {
            cid: gc,
            drawable: window,
            value_list: Cow::Owned(CreateGCAux::default()),
        }
        .serialize(),
    )?;
    conn.send_checked(MapWindowRequest { window }.serialize())?;

    let mut size = (width, height);
    let mut pixels: Vec<u32> = Vec::new();
    let mut queue: Vec<Event> = Vec::new();

    loop {
        conn.pump(&mut queue)?;

        // Track size changes so the closure sees the current window size.
        for event in &queue {
            if let Event::Resized { width, height } = *event {
                size = (width, height);
            }
        }

        let (w, h) = size;
        if w == 0 || h == 0 {
            thread::sleep(Duration::from_millis(16));
            continue;
        }
        if pixels.len() != pixel_count(w, h) {
            pixels.clear();
            pixels.resize(pixel_count(w, h), CLEAR);
        }
        pixels.fill(CLEAR);

        let events = std::mem::take(&mut queue);
        let flow = on_frame(Frame {
            width: w,
            height: h,
            pixels: &mut pixels,
            events: &events,
        });

        conn.put_image(window, gc, w, h, &pixels)?;

        if flow == ControlFlow::Exit {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(8));
    }
}

impl Connection {
    /// Connects to `$DISPLAY`, completes the handshake, and loads the keyboard
    /// mapping and the `WM_DELETE_WINDOW` atom.
    fn open() -> Result<Self, PlatformError> {
        let display = std::env::var("DISPLAY").ok();
        let parsed = parse_display(display.as_deref())
            .map_err(|e| PlatformError::Os(format!("cannot parse DISPLAY: {e}")))?;

        let mut last_error = String::from("no X server address to try");
        for address in parsed.connect_instruction() {
            let (mut stream, family, auth_address) = match &address {
                ConnectAddress::Socket(path) => match UnixStream::connect(path) {
                    Ok(s) => (Stream::Unix(s), Family::LOCAL, hostname().into_bytes()),
                    Err(e) => {
                        last_error = format!("{path}: {e}");
                        continue;
                    }
                },
                ConnectAddress::Hostname(host, port) => match TcpStream::connect((*host, *port)) {
                    Ok(s) => (Stream::Tcp(s), Family::INTERNET, host.as_bytes().to_vec()),
                    Err(e) => {
                        last_error = format!("{host}:{port}: {e}");
                        continue;
                    }
                },
                _ => continue,
            };

            let setup = match handshake(&mut stream, family, &auth_address, parsed.display) {
                Ok(setup) => setup,
                Err(e) => {
                    last_error = e;
                    continue;
                }
            };
            stream
                .set_nonblocking(true)
                .map_err(|e| PlatformError::Os(format!("set nonblocking: {e}")))?;

            let mut conn = Connection {
                stream,
                min_keycode: setup.min_keycode,
                setup,
                seq: 0,
                inbuf: Vec::new(),
                errors: Vec::new(),
                keysyms_per_keycode: 0,
                keysyms: Vec::new(),
                wm_delete: 0,
            };
            conn.load_keyboard()?;
            conn.wm_delete = conn.intern_atom(b"WM_DELETE_WINDOW")?;
            return Ok(conn);
        }
        Err(PlatformError::Os(format!(
            "cannot connect to X server: {last_error}"
        )))
    }

    fn create_window(
        &mut self,
        title: &str,
        width: u32,
        height: u32,
    ) -> Result<u32, PlatformError> {
        let screen = self
            .setup
            .roots
            .first()
            .ok_or_else(|| PlatformError::Os("X11 server has no screens".into()))?;
        let (root, visual) = (screen.root, screen.root_visual);
        let window = self.setup.resource_id_base;
        let mask = EventMask::KEY_PRESS
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::POINTER_MOTION
            | EventMask::STRUCTURE_NOTIFY;
        let width =
            u16::try_from(width).map_err(|_| PlatformError::Os("window too wide".into()))?;
        let height =
            u16::try_from(height).map_err(|_| PlatformError::Os("window too tall".into()))?;

        self.send_checked(
            CreateWindowRequest {
                depth: 24,
                wid: window,
                parent: root,
                x: 0,
                y: 0,
                width,
                height,
                border_width: 0,
                class: WindowClass::INPUT_OUTPUT,
                visual,
                value_list: Cow::Owned(CreateWindowAux {
                    background_pixel: Some(CLEAR),
                    event_mask: Some(mask),
                    ..Default::default()
                }),
            }
            .serialize(),
        )?;

        let net_wm_name = self.intern_atom(b"_NET_WM_NAME")?;
        let utf8 = self.intern_atom(b"UTF8_STRING")?;
        self.send_checked(
            ChangePropertyRequest {
                mode: PropMode::REPLACE,
                window,
                property: net_wm_name,
                type_: utf8,
                format: 8,
                data_len: title.len() as u32,
                data: Cow::Borrowed(title.as_bytes()),
            }
            .serialize(),
        )?;

        let protocols = self.intern_atom(b"WM_PROTOCOLS")?;
        self.send_checked(
            ChangePropertyRequest {
                mode: PropMode::REPLACE,
                window,
                property: protocols,
                type_: ATOM_TYPE,
                format: 32,
                data_len: 1,
                data: Cow::Owned(self.wm_delete.to_le_bytes().to_vec()),
            }
            .serialize(),
        )?;
        Ok(window)
    }

    /// Reads the keyboard mapping so keycodes can be turned into keysyms.
    fn load_keyboard(&mut self) -> Result<(), PlatformError> {
        let count = self
            .setup
            .max_keycode
            .saturating_sub(self.min_keycode)
            .saturating_add(1);
        let seq = self.send(
            GetKeyboardMappingRequest {
                first_keycode: self.min_keycode,
                count,
            }
            .serialize(),
        )?;
        let reply = self.wait_reply(seq)?;
        let (mapping, _) = GetKeyboardMappingReply::try_parse(&reply)
            .map_err(|e| PlatformError::Os(format!("keyboard mapping: {e:?}")))?;
        self.keysyms_per_keycode = usize::from(mapping.keysyms_per_keycode);
        self.keysyms = mapping.keysyms;
        Ok(())
    }

    fn intern_atom(&mut self, name: &[u8]) -> Result<u32, PlatformError> {
        let seq = self.send(
            InternAtomRequest {
                only_if_exists: false,
                name: Cow::Borrowed(name),
            }
            .serialize(),
        )?;
        let reply = self.wait_reply(seq)?;
        let (atom, _) = InternAtomReply::try_parse(&reply)
            .map_err(|e| PlatformError::Os(format!("InternAtom: {e:?}")))?;
        Ok(atom.atom)
    }

    /// Presents `pixels` by sending it in row bands, each within one request.
    fn put_image(
        &mut self,
        window: u32,
        gc: u32,
        w: u32,
        h: u32,
        pixels: &[u32],
    ) -> Result<(), PlatformError> {
        let row_bytes = w as usize * 4;
        let max_request = usize::from(self.setup.maximum_request_length) * 4;
        let budget = max_request.saturating_sub(PUT_IMAGE_HEADER);
        if row_bytes > budget {
            return Err(PlatformError::Os(format!(
                "window width {w} exceeds the X server's request size limit"
            )));
        }
        let rows_per_band = (budget / row_bytes).max(1);

        let w_px = w as usize;
        let mut row = 0usize;
        while row < h as usize {
            let band = rows_per_band.min(h as usize - row);
            let mut data = Vec::with_capacity(band * row_bytes);
            for px in &pixels[row * w_px..(row + band) * w_px] {
                data.extend_from_slice(&px.to_le_bytes());
            }
            self.send(
                PutImageRequest {
                    format: ImageFormat::Z_PIXMAP,
                    drawable: window,
                    gc,
                    width: w as u16,
                    height: band as u16,
                    dst_x: 0,
                    dst_y: row as i16,
                    left_pad: 0,
                    depth: 24,
                    data: Cow::Owned(data),
                }
                .serialize(),
            )?;
            row += band;
        }
        Ok(())
    }

    /// Writes one serialized request and returns its sequence number.
    fn send<const N: usize, F>(
        &mut self,
        request: ([Cow<'_, [u8]>; N], F),
    ) -> Result<u16, PlatformError> {
        let (parts, _fds) = request;
        for part in &parts {
            self.write_all(part)?;
        }
        self.seq = self.seq.wrapping_add(1);
        Ok(self.seq)
    }

    /// Sends a request that has no reply, then waits for the server to finish
    /// processing it. An error reported for it becomes an `Err`, so a failed
    /// setup step (such as CreateWindow) is not silently ignored.
    fn send_checked<const N: usize, F>(
        &mut self,
        request: ([Cow<'_, [u8]>; N], F),
    ) -> Result<(), PlatformError> {
        let seq = self.send(request)?;
        // X replies to requests in order, so a reply to this fence means every
        // earlier request has been handled and any error for `seq` has arrived.
        let fence = self.send(GetInputFocusRequest.serialize())?;
        self.wait_reply(fence)?;
        if let Some(pos) = self.errors.iter().position(|(s, _)| *s == seq) {
            let (_, msg) = self.errors.remove(pos);
            return Err(PlatformError::Os(msg));
        }
        Ok(())
    }

    fn write_all(&mut self, mut bytes: &[u8]) -> Result<(), PlatformError> {
        while !bytes.is_empty() {
            match self.stream.write(bytes) {
                Ok(0) => return Err(PlatformError::Os("X server closed the connection".into())),
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1))
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(PlatformError::Os(format!("write to X server: {e}"))),
            }
        }
        Ok(())
    }

    /// Reads whatever the server has sent, appending the events it implies to `out`.
    fn pump(&mut self, out: &mut Vec<Event>) -> Result<(), PlatformError> {
        self.fill()?;
        while self.inbuf.len() >= PACKET {
            let packet = self.take_packet();
            match packet[0] {
                // Replies are only awaited in `wait_reply`; a reply here is stale.
                1 => {}
                0 => {
                    let (seq, msg) = describe_error(&packet);
                    self.errors.push((seq, msg));
                }
                _ => self.translate(&packet, out),
            }
        }
        // Errors nobody is waiting for would otherwise accumulate forever.
        self.errors.clear();
        Ok(())
    }

    /// Reads from the socket into `inbuf` without blocking.
    fn fill(&mut self) -> Result<(), PlatformError> {
        let mut chunk = [0u8; 4096];
        loop {
            match self.stream.read(&mut chunk) {
                Ok(0) => return Err(PlatformError::Os("X server closed the connection".into())),
                Ok(n) => self.inbuf.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(PlatformError::Os(format!("read from X server: {e}"))),
            }
        }
    }

    /// Removes and returns the next fixed-size packet. The caller has checked
    /// that at least `PACKET` bytes are buffered.
    fn take_packet(&mut self) -> [u8; PACKET] {
        let mut packet = [0u8; PACKET];
        packet.copy_from_slice(&self.inbuf[..PACKET]);
        self.inbuf.drain(..PACKET);
        packet
    }

    /// Waits for the reply to request `seq`, skipping events and other packets.
    /// An error for `seq` is returned as an `Err`.
    fn wait_reply(&mut self, seq: u16) -> Result<Vec<u8>, PlatformError> {
        loop {
            if self.inbuf.len() >= PACKET {
                if self.inbuf[0] == 1 {
                    let words =
                        u32::from_le_bytes(self.inbuf[4..8].try_into().expect("4 bytes")) as usize;
                    let total = PACKET + words * 4;
                    if self.inbuf.len() >= total {
                        let packet: Vec<u8> = self.inbuf.drain(..total).collect();
                        if u16::from_le_bytes([packet[2], packet[3]]) == seq {
                            return Ok(packet);
                        }
                        continue;
                    }
                } else {
                    let packet = self.take_packet();
                    if packet[0] == 0 {
                        let (err_seq, msg) = describe_error(&packet);
                        if err_seq == seq {
                            return Err(PlatformError::Os(msg));
                        }
                        self.errors.push((err_seq, msg));
                    } else {
                        self.translate(&packet, &mut Vec::new());
                    }
                    continue;
                }
            }
            self.fill()?;
            if self.inbuf.len() < PACKET {
                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// Converts one event packet into zero or more [`Event`]s.
    fn translate(&self, p: &[u8; PACKET], out: &mut Vec<Event>) {
        let code = p[0] & 0x7f;
        let i16_at = |at: usize| i16::from_le_bytes([p[at], p[at + 1]]) as i32;
        let u16_at = |at: usize| u16::from_le_bytes([p[at], p[at + 1]]);
        match code {
            // KeyPress: keycode in byte 1, modifier state at 28.
            2 => {
                let state = u16_at(28);
                let shift = state & SHIFT_MASK != 0;
                let ctrl = state & CONTROL_MASK != 0;
                let alt = state & MOD1_MASK != 0;
                let sym = self.keysym(p[1], shift);
                if !ctrl && !alt {
                    if let Some(ch) = printable(sym) {
                        out.push(Event::Text(ch));
                        return;
                    }
                }
                if let Some(code) = virtual_key(sym) {
                    out.push(Event::KeyDown {
                        code,
                        shift,
                        ctrl,
                        alt,
                    });
                }
            }
            // ButtonPress (4) and ButtonRelease (5): button in byte 1, position at 24/26.
            4 | 5 => {
                let (x, y) = (i16_at(24), i16_at(26));
                let press = code == 4;
                match p[1] {
                    1..=3 => {
                        let button = match p[1] {
                            1 => MouseButton::Primary,
                            2 => MouseButton::Middle,
                            _ => MouseButton::Secondary,
                        };
                        out.push(if press {
                            Event::PointerDown { x, y, button }
                        } else {
                            Event::PointerUp { x, y, button }
                        });
                    }
                    // Buttons 4 and 5 are the wheel: up (away from the user) is positive.
                    4 | 5 if press => {
                        let delta = if p[1] == 4 { 1.0 } else { -1.0 };
                        out.push(Event::Wheel { x, y, delta });
                    }
                    _ => {}
                }
            }
            6 => out.push(Event::PointerMoved {
                x: i16_at(24),
                y: i16_at(26),
            }),
            // ConfigureNotify: width at 20, height at 22.
            22 => out.push(Event::Resized {
                width: u32::from(u16_at(20)),
                height: u32::from(u16_at(22)),
            }),
            // ClientMessage: format in byte 1, first data word (an atom) at 12.
            33 => {
                let atom = u32::from_le_bytes([p[12], p[13], p[14], p[15]]);
                if p[1] == 32 && atom == self.wm_delete {
                    out.push(Event::CloseRequested);
                }
            }
            _ => {}
        }
    }

    /// The keysym for `keycode` at the shift level, falling back to the
    /// unshifted level when the shifted one is empty.
    fn keysym(&self, keycode: u8, shift: bool) -> u32 {
        if keycode < self.min_keycode || self.keysyms_per_keycode == 0 {
            return 0;
        }
        let base = usize::from(keycode - self.min_keycode) * self.keysyms_per_keycode;
        let level = if shift && self.keysyms_per_keycode > 1 {
            1
        } else {
            0
        };
        self.keysyms
            .get(base + level)
            .copied()
            .filter(|&s| s != 0)
            .or_else(|| self.keysyms.get(base).copied())
            .unwrap_or(0)
    }
}

/// Runs the X11 connection handshake and returns the server's setup.
fn handshake(
    stream: &mut Stream,
    family: Family,
    address: &[u8],
    display: u16,
) -> Result<Setup, String> {
    let (mut connect, request) =
        Connect::new(family, address, display).map_err(|e| format!("authorization: {e}"))?;
    stream
        .write_all(&request)
        .map_err(|e| format!("send setup: {e}"))?;
    loop {
        let buf = connect.buffer();
        let n = stream.read(buf).map_err(|e| format!("read setup: {e}"))?;
        if n == 0 {
            return Err("X server closed during setup".into());
        }
        if connect.advance(n) {
            break;
        }
    }
    connect
        .into_setup()
        .map_err(|e| format!("setup refused: {e}"))
}

/// The machine's host name, as X authorization entries record it.
fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "localhost".into())
}

/// The sequence number and a readable description of an error packet.
fn describe_error(p: &[u8; PACKET]) -> (u16, String) {
    let seq = u16::from_le_bytes([p[2], p[3]]);
    let msg = format!(
        "X11 error code {} (request sequence {seq}, major opcode {}, bad value {})",
        p[1],
        p[10],
        u32::from_le_bytes([p[4], p[5], p[6], p[7]])
    );
    (seq, msg)
}

/// The character a keysym types, if it is printable.
fn printable(sym: u32) -> Option<char> {
    match sym {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(sym),
        0x0100_0100..=0x0110_ffff => char::from_u32(sym - 0x0100_0000),
        _ => None,
    }
    .filter(|c| !c.is_control())
}

/// The virtual-key code for a non-printable keysym, if one is mapped.
fn virtual_key(sym: u32) -> Option<u32> {
    KEYSYM_TABLE
        .iter()
        .find(|(s, _)| *s == sym)
        .map(|(_, vk)| *vk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_keysyms_are_printable() {
        assert_eq!(printable(0x61), Some('a'));
        assert_eq!(printable(0x20), Some(' '));
        assert_eq!(printable(0x0100_0000 + 0x1F600), Some('\u{1F600}'));
        assert_eq!(printable(0xff0d), None); // Return is a key, not text
    }

    #[test]
    fn control_keysyms_map_to_virtual_keys() {
        assert_eq!(virtual_key(0xff0d), Some(0x0D));
        assert_eq!(virtual_key(0xff09), Some(0x09));
        assert_eq!(virtual_key(0xff51), Some(0x25));
        assert_eq!(virtual_key(0x61), None);
    }

    #[test]
    fn error_packets_report_their_sequence() {
        let mut p = [0u8; PACKET];
        p[1] = 3; // BadWindow
        p[2..4].copy_from_slice(&42u16.to_le_bytes());
        let (seq, msg) = describe_error(&p);
        assert_eq!(seq, 42);
        assert!(msg.contains("code 3"));
    }
}
