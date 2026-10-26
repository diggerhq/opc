//! The terminal's modes: raw input, bracketed paste, focus events and the
//! kitty keyboard protocol. Whatever happens, the terminal is put back: on
//! return, on error, on drop and from the panic hook. The alternate screen
//! is never used.

use std::io::Write;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::{cursor, execute, terminal};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static KITTY: AtomicBool = AtomicBool::new(false);
static TITLE: AtomicBool = AtomicBool::new(false);
static HOOK: Once = Once::new();

/// Holds the terminal in the session screen's modes until dropped.
pub struct Guard {
    _private: (),
}

/// Raw mode and the rest. Installs the panic hook once.
pub fn enter() -> std::io::Result<Guard> {
    HOOK.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            prev(info);
        }));
    });
    modes_on()?;
    Ok(Guard { _private: () })
}

fn modes_on() -> std::io::Result<()> {
    terminal::enable_raw_mode()?;
    ACTIVE.store(true, Ordering::SeqCst);
    let mut out = std::io::stdout();
    execute!(out, EnableBracketedPaste, EnableFocusChange)?;
    if matches!(terminal::supports_keyboard_enhancement(), Ok(true)) {
        execute!(out, PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES))?;
        KITTY.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Sets the terminal's title, saving the old one the first time.
pub fn title(t: &str) {
    let mut out = std::io::stdout();
    if !TITLE.swap(true, Ordering::SeqCst) {
        let _ = out.write_all(super::notify::push_title().as_bytes());
    }
    let _ = out.write_all(super::notify::set_title(t).as_bytes());
    let _ = out.flush();
}

/// Puts the terminal back. Safe to call any number of times.
pub fn restore() {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut out = std::io::stdout();
    if KITTY.swap(false, Ordering::SeqCst) {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(out, DisableBracketedPaste, DisableFocusChange, cursor::Show);
    let _ = out.write_all(b"\x1b[?2026l\x1b[0m");
    let _ = terminal::disable_raw_mode();
    let _ = out.flush();
}

/// Hands the terminal to another program (the pager, the editor, a shell,
/// the shell's job control) and takes it back after `f`.
pub fn handed_over<T>(f: impl FnOnce() -> T) -> T {
    restore();
    let r = f();
    let _ = modes_on();
    r
}

/// Takes the terminal back after [`restore`] (an async hand-over, such as
/// `/shell`).
pub fn resume() {
    let _ = modes_on();
}

/// `Ctrl+Z`: stop as any program does, and come back on `fg`.
pub fn suspend_self() {
    handed_over(|| {
        let pid = std::process::id().to_string();
        let _ = std::process::Command::new("kill").args(["-TSTP", &pid]).status();
    });
}

impl Drop for Guard {
    fn drop(&mut self) {
        restore();
        if TITLE.swap(false, Ordering::SeqCst) {
            let mut out = std::io::stdout();
            let _ = out.write_all(super::notify::pop_title().as_bytes());
            let _ = out.flush();
        }
    }
}
