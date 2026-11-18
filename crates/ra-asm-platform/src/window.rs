//! The host of a console's program on Windows: a window for its picture, the keyboard and an
//! XInput gamepad for its controllers, and a wave-out device for its sound. It is written on the
//! system's own calls, so the engine takes no windowing or audio library with it.
//!
//! The keyboard's layout: the arrows are the direction pad; `Z` cross, `X` circle, `A` square,
//! `S` triangle; `Q` and `W` the upper shoulder buttons, `1` and `2` the lower; `Enter` start and
//! `Backspace` select. A gamepad's buttons are the pad's by their places.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::host::{Audio, Frame, Input, Video};
use crate::pad;

#[repr(C)]
struct WndClass {
    style: u32,
    procedure: unsafe extern "system" fn(isize, u32, usize, isize) -> isize,
    class_extra: i32,
    window_extra: i32,
    instance: isize,
    icon: isize,
    cursor: isize,
    background: isize,
    menu: *const u16,
    name: *const u16,
}

#[repr(C)]
#[derive(Default)]
struct Msg {
    window: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    point: [i32; 2],
    private: u32,
}

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct BitmapInfo {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bits: u16,
    compression: u32,
    image_size: u32,
    x_per_meter: i32,
    y_per_meter: i32,
    colours_used: u32,
    colours_important: u32,
}

#[repr(C)]
struct WaveFormat {
    tag: u16,
    channels: u16,
    rate: u32,
    bytes_per_second: u32,
    block_align: u16,
    bits: u16,
    extra: u16,
}

#[repr(C)]
struct WaveHeader {
    data: *mut u8,
    length: u32,
    recorded: u32,
    user: usize,
    flags: u32,
    loops: u32,
    next: *mut WaveHeader,
    reserved: usize,
}

#[repr(C)]
#[derive(Default)]
struct XInputState {
    packet: u32,
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    thumbs: [i16; 4],
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassW(class: *const WndClass) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: isize,
        menu: isize,
        instance: isize,
        parameter: *const c_void,
    ) -> isize;
    fn DefWindowProcW(window: isize, message: u32, wparam: usize, lparam: isize) -> isize;
    fn PeekMessageW(message: *mut Msg, window: isize, min: u32, max: u32, remove: u32) -> i32;
    fn TranslateMessage(message: *const Msg) -> i32;
    fn DispatchMessageW(message: *const Msg) -> isize;
    fn GetDC(window: isize) -> isize;
    fn ReleaseDC(window: isize, context: isize) -> i32;
    fn GetClientRect(window: isize, rect: *mut Rect) -> i32;
    fn AdjustWindowRect(rect: *mut Rect, style: u32, menu: i32) -> i32;
    fn ShowWindow(window: isize, command: i32) -> i32;
    fn DestroyWindow(window: isize) -> i32;
    fn GetAsyncKeyState(key: i32) -> i16;
    fn GetForegroundWindow() -> isize;
    fn LoadCursorW(instance: isize, name: *const u16) -> isize;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn StretchDIBits(
        context: isize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        source_x: i32,
        source_y: i32,
        source_width: i32,
        source_height: i32,
        bits: *const c_void,
        info: *const BitmapInfo,
        usage: u32,
        operation: u32,
    ) -> i32;
    fn SetStretchBltMode(context: isize, mode: i32) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
    fn LoadLibraryW(name: *const u16) -> isize;
    fn GetProcAddress(module: isize, name: *const u8) -> *const c_void;
}

#[link(name = "winmm")]
unsafe extern "system" {
    fn waveOutOpen(device: *mut isize, id: u32, format: *const WaveFormat, callback: usize, instance: usize, flags: u32) -> u32;
    fn waveOutPrepareHeader(device: isize, header: *mut WaveHeader, size: u32) -> u32;
    fn waveOutWrite(device: isize, header: *mut WaveHeader, size: u32) -> u32;
    fn waveOutUnprepareHeader(device: isize, header: *mut WaveHeader, size: u32) -> u32;
    fn waveOutReset(device: isize) -> u32;
    fn waveOutClose(device: isize) -> u32;
}

const WM_CLOSE: u32 = 0x0010;
const WS_OVERLAPPEDWINDOW: u32 = 0x00cf_0000;
const WAVE_DONE: u32 = 1;
/// The most sound queued at the device before more is dropped: a run slower than the console
/// would otherwise fall ever further behind its own picture.
const QUEUED_MOST: usize = 12;

/// The person closed the window. One program runs in a process, so one flag serves.
static CLOSED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn procedure(window: isize, message: u32, wparam: usize, lparam: isize) -> isize {
    if message == WM_CLOSE {
        CLOSED.store(true, Ordering::Relaxed);
        return 0;
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A buffer of sound at the device: its header, which the device holds a pointer to, and its
/// samples, both kept where they are until the device is done with them.
struct Queued {
    header: Box<WaveHeader>,
    _samples: Vec<i16>,
}

pub struct Window {
    window: isize,
    /// When the last frame was shown and the messages were last read.
    shown: Instant,
    pumped: Instant,
    /// The frame's time, by the rate the frames come at.
    frame_time: Duration,
    wave: isize,
    wave_rate: u32,
    queued: VecDeque<Queued>,
    /// `XInputGetState`, when the system has the library.
    xinput: Option<unsafe extern "system" fn(u32, *mut XInputState) -> u32>,
}

impl Window {
    /// A window titled `title`, sized for a console's picture; `None` when the system gives none
    /// (a session without a desktop).
    pub fn open(title: &str) -> Option<Self> {
        CLOSED.store(false, Ordering::Relaxed);
        let class_name = wide("RhipperConsole");
        let style = WS_OVERLAPPEDWINDOW;
        let mut rect = Rect { left: 0, top: 0, right: 640, bottom: 480 };
        // SAFETY: the calls take pointers to locals that outlive them, and the class's procedure
        // is a function of this module, which the process never unloads.
        let window = unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class = WndClass {
                style: 0,
                procedure,
                class_extra: 0,
                window_extra: 0,
                instance,
                icon: 0,
                // IDC_ARROW.
                cursor: LoadCursorW(0, 32512 as *const u16),
                background: 0,
                menu: std::ptr::null(),
                name: class_name.as_ptr(),
            };
            // A second window of the process finds the class there already.
            RegisterClassW(&raw const class);
            AdjustWindowRect(&raw mut rect, style, 0);
            let window = CreateWindowExW(
                0,
                class_name.as_ptr(),
                wide(title).as_ptr(),
                style,
                i32::MIN,
                i32::MIN,
                rect.right - rect.left,
                rect.bottom - rect.top,
                0,
                0,
                instance,
                std::ptr::null(),
            );
            if window == 0 {
                return None;
            }
            ShowWindow(window, 5);
            window
        };
        let xinput = ["xinput1_4.dll", "xinput1_3.dll", "xinput9_1_0.dll"].iter().find_map(|name| {
            // SAFETY: the library's function has the signature its documentation gives.
            unsafe {
                let module = LoadLibraryW(wide(name).as_ptr());
                let found = if module == 0 { std::ptr::null() } else { GetProcAddress(module, c"XInputGetState".as_ptr().cast()) };
                (!found.is_null())
                    .then(|| std::mem::transmute::<*const c_void, unsafe extern "system" fn(u32, *mut XInputState) -> u32>(found))
            }
        });
        let now = Instant::now();
        Some(Self {
            window,
            shown: now,
            pumped: now,
            frame_time: Duration::from_nanos(1_000_000_000 / 60),
            wave: 0,
            wave_rate: 0,
            queued: VecDeque::new(),
            xinput,
        })
    }

    /// Reads the window's messages; `false` once the person closed it.
    fn pump(&mut self) -> bool {
        self.pumped = Instant::now();
        let mut message = Msg::default();
        // SAFETY: the message is a local the calls fill and read.
        unsafe {
            while PeekMessageW(&raw mut message, 0, 0, 0, 1) != 0 {
                TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
        !CLOSED.load(Ordering::Relaxed)
    }

    fn key(key: i32) -> bool {
        // SAFETY: the call reads the keyboard's state.
        unsafe { GetAsyncKeyState(key).cast_unsigned() & 0x8000 != 0 }
    }

    fn keyboard(&self) -> u16 {
        // SAFETY: the call names the window in front.
        if unsafe { GetForegroundWindow() } != self.window {
            return 0;
        }
        const KEYS: [(i32, u16); 14] = [
            (0x26, pad::UP),
            (0x28, pad::DOWN),
            (0x25, pad::LEFT),
            (0x27, pad::RIGHT),
            (0x5a, pad::CROSS),
            (0x58, pad::CIRCLE),
            (0x41, pad::SQUARE),
            (0x53, pad::TRIANGLE),
            (0x51, pad::L1),
            (0x57, pad::R1),
            (0x31, pad::L2),
            (0x32, pad::R2),
            (0x0d, pad::START),
            (0x08, pad::SELECT),
        ];
        KEYS.iter().filter(|(key, _)| Self::key(*key)).fold(0, |buttons, (_, button)| buttons | button)
    }

    /// The gamepad `index` as a pad, when one is connected.
    fn gamepad(&self, index: u32) -> Option<pad::State> {
        let mut state = XInputState::default();
        // SAFETY: the function fills the state it is handed.
        if unsafe { (self.xinput?)(index, &raw mut state) } != 0 {
            return None;
        }
        const BUTTONS: [(u16, u16); 14] = [
            (0x0001, pad::UP),
            (0x0002, pad::DOWN),
            (0x0004, pad::LEFT),
            (0x0008, pad::RIGHT),
            (0x0010, pad::START),
            (0x0020, pad::SELECT),
            (0x0040, pad::L3),
            (0x0080, pad::R3),
            (0x0100, pad::L1),
            (0x0200, pad::R1),
            (0x1000, pad::CROSS),
            (0x2000, pad::CIRCLE),
            (0x4000, pad::SQUARE),
            (0x8000, pad::TRIANGLE),
        ];
        let mut buttons = BUTTONS.iter().filter(|(bit, _)| state.buttons & bit != 0).fold(0, |all, (_, button)| all | button);
        if state.left_trigger > 64 {
            buttons |= pad::L2;
        }
        if state.right_trigger > 64 {
            buttons |= pad::R2;
        }
        // The pad's sticks run from 0 to 255 with down and right the high end.
        let axis = |v: i16, flip: bool| {
            let v = if flip { -(i32::from(v) + 1) } else { i32::from(v) };
            ((v + 0x8000) >> 8) as u8
        };
        let [lx, ly, rx, ry] = state.thumbs;
        Some(pad::State { buttons, sticks: [axis(rx, false), axis(ry, true), axis(lx, false), axis(ly, true)] })
    }

    fn close_wave(&mut self) {
        if self.wave != 0 {
            // SAFETY: the device is open, and the reset hands every buffer back before the close.
            unsafe {
                waveOutReset(self.wave);
                for mut buffer in self.queued.drain(..) {
                    waveOutUnprepareHeader(self.wave, &raw mut *buffer.header, size_of::<WaveHeader>() as u32);
                }
                waveOutClose(self.wave);
            }
            self.wave = 0;
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.close_wave();
        // SAFETY: the window is this value's own.
        unsafe {
            DestroyWindow(self.window);
        }
    }
}

/// The window as each part of the host: one window is the picture, the input and the sound.
pub struct Shared(pub Rc<RefCell<Window>>);

impl Video for Shared {
    fn present(&mut self, frame: &Frame) -> bool {
        let mut window = self.0.borrow_mut();
        if !window.pump() {
            return false;
        }
        let info = BitmapInfo {
            size: size_of::<BitmapInfo>() as u32,
            width: frame.width.cast_signed(),
            // A negative height is a picture whose first row is the top one.
            height: -frame.height.cast_signed(),
            planes: 1,
            bits: 32,
            compression: 0,
            image_size: 0,
            x_per_meter: 0,
            y_per_meter: 0,
            colours_used: 0,
            colours_important: 0,
        };
        let mut client = Rect::default();
        // SAFETY: the pixels are `width * height` words, as the header says, and the device
        // context is released where it was taken.
        unsafe {
            let context = GetDC(window.window);
            GetClientRect(window.window, &raw mut client);
            // COLORONCOLOR: the pixels as they are, not averaged.
            SetStretchBltMode(context, 3);
            StretchDIBits(
                context,
                0,
                0,
                client.right,
                client.bottom,
                0,
                0,
                frame.width.cast_signed(),
                frame.height.cast_signed(),
                frame.pixels.as_ptr().cast(),
                &raw const info,
                0,
                0x00cc_0020,
            );
            ReleaseDC(window.window, context);
        }
        // A run faster than the console waits for the frame's time.
        let due = window.shown + window.frame_time;
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
            window.shown = due;
        } else {
            window.shown = now;
        }
        true
    }

    fn idle(&mut self) -> bool {
        let mut window = self.0.borrow_mut();
        window.pumped.elapsed() < Duration::from_millis(30) || window.pump()
    }
}

impl Input for Shared {
    fn pad(&mut self, port: usize) -> Option<pad::State> {
        let window = self.0.borrow();
        let gamepad = window.gamepad(port as u32);
        // The keyboard is the first port's, with the gamepad's buttons when there is one.
        if port == 0 {
            let mut state = gamepad.unwrap_or_default();
            state.buttons |= window.keyboard();
            return Some(state);
        }
        gamepad
    }
}

impl Audio for Shared {
    fn play(&mut self, samples: &[[i16; 2]], rate: u32) {
        let mut window = self.0.borrow_mut();
        if samples.is_empty() {
            return;
        }
        if window.wave_rate != rate {
            window.close_wave();
            let format = WaveFormat { tag: 1, channels: 2, rate, bytes_per_second: rate * 4, block_align: 4, bits: 16, extra: 0 };
            let mut device = 0;
            // SAFETY: the format is a local the call reads; the mapper picks the device.
            if unsafe { waveOutOpen(&raw mut device, u32::MAX, &raw const format, 0, 0, 0) } != 0 {
                return;
            }
            window.wave = device;
            window.wave_rate = rate;
            window.frame_time = Duration::from_nanos(1_000_000_000 * samples.len() as u64 / u64::from(rate));
        }
        let size = size_of::<WaveHeader>() as u32;
        // The buffers the device finished are taken back.
        while window.queued.front().is_some_and(|b| b.header.flags & WAVE_DONE != 0) {
            if let Some(mut done) = window.queued.pop_front() {
                // SAFETY: the device is done with the buffer.
                unsafe {
                    waveOutUnprepareHeader(window.wave, &raw mut *done.header, size);
                }
            }
        }
        if window.queued.len() >= QUEUED_MOST {
            return;
        }
        let mut flat: Vec<i16> = samples.iter().flatten().copied().collect();
        let mut header = Box::new(WaveHeader {
            data: flat.as_mut_ptr().cast(),
            length: (flat.len() * 2) as u32,
            recorded: 0,
            user: 0,
            flags: 0,
            loops: 0,
            next: std::ptr::null_mut(),
            reserved: 0,
        });
        // SAFETY: the header and the samples stay where they are, in the queue, until the device
        // marks the header done.
        let written = unsafe {
            waveOutPrepareHeader(window.wave, &raw mut *header, size) == 0 && waveOutWrite(window.wave, &raw mut *header, size) == 0
        };
        if written {
            window.queued.push_back(Queued { header, _samples: flat });
        }
    }
}
