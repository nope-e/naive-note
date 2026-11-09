//! Read WM_POINTER synchronously; defer only owned samples to the application.

use std::{cell::RefCell, rc::Rc};

use anyhow::{Context, Result, bail};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tablet_input_windows::{PenSample, ink::WindowsInk};
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT},
        WindowsAndMessaging::{
            GetMessageExtraInfo, MSG, WM_ACTIVATEAPP, WM_CANCELMODE, WM_CAPTURECHANGED,
            WM_KILLFOCUS, WM_MOUSEFIRST, WM_MOUSELAST,
        },
    },
};
use winit::{
    event_loop::EventLoopBuilder, platform::windows::EventLoopBuilderExtWindows, window::Window,
};

#[derive(Default)]
struct State {
    ink: Option<WindowsInk>,
    pending: Vec<PenEvent>,
    cancel_requested: bool,
}

#[derive(Clone, Default)]
pub struct PenInput(Rc<RefCell<State>>);

#[derive(Clone, Copy)]
pub struct PenEvent {
    pub sample: PenSample,
    pub modifiers: egui::Modifiers,
}

impl PenInput {
    pub fn install<T>(&self, builder: &mut EventLoopBuilder<T>) {
        let shared = self.clone();
        builder.with_msg_hook(move |raw| {
            // SAFETY: winit provides a live Win32 MSG for the callback duration.
            let message = unsafe { &*raw.cast::<MSG>() };
            let mut state = shared.0.borrow_mut();
            let Some(ink) = state.ink.as_mut() else {
                return false;
            };
            if message.hwnd != ink.hwnd() {
                return false;
            }
            if matches!(
                message.message,
                WM_KILLFOCUS | WM_CANCELMODE | WM_CAPTURECHANGED
            ) || (message.message == WM_ACTIVATEAPP && message.wParam.0 == 0)
            {
                let samples = ink.cancel_all();
                enqueue(&mut state.pending, samples, read_modifiers());
                state.cancel_requested = true;
                return false;
            }
            // https://learn.microsoft.com/en-us/windows/win32/tablet/system-events-and-mouse-messages
            // Filter only pen promotion; touch has a separate 0x80 bit.
            if (WM_MOUSEFIRST..=WM_MOUSELAST).contains(&message.message)
                // SAFETY: this reads the current UI thread's message metadata.
                && is_pen_mouse(unsafe { GetMessageExtraInfo() }.0 as usize)
            {
                return true;
            }
            let result = ink.handle_message(
                message.hwnd,
                message.message,
                message.wParam,
                message.lParam,
            );
            if let Some(error) = result.error {
                log::warn!("Windows Ink: {error}");
            }
            // Snapshot before another GetMessage/PeekMessage can change the
            // thread's keyboard state. Coalesced history uses this message's keys.
            enqueue(&mut state.pending, result.samples, read_modifiers());
            result.handled
        });
    }

    pub fn attach(&self, window: &Window) -> Result<()> {
        let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
            bail!("Windows Ink requires a Win32 window");
        };
        let ink =
            WindowsInk::new(HWND(handle.hwnd.get() as *mut _)).context("attach Windows Ink")?;
        self.0.borrow_mut().ink = Some(ink);
        Ok(())
    }

    pub fn take_samples(&self) -> Vec<PenEvent> {
        std::mem::take(&mut self.0.borrow_mut().pending)
    }

    pub fn take_cancel_request(&self) -> bool {
        std::mem::take(&mut self.0.borrow_mut().cancel_requested)
    }

    pub fn cancel(&self) {
        let mut state = self.0.borrow_mut();
        if let Some(ink) = state.ink.as_mut() {
            let samples = ink.cancel_all();
            enqueue(&mut state.pending, samples, read_modifiers());
        }
    }

    pub fn detach(&self) {
        self.cancel();
        let mut state = self.0.borrow_mut();
        state.ink = None;
        state.pending.clear();
        state.cancel_requested = false;
    }
}

fn enqueue(pending: &mut Vec<PenEvent>, samples: Vec<PenSample>, modifiers: egui::Modifiers) {
    pending.extend(
        samples
            .into_iter()
            .map(|sample| PenEvent { sample, modifiers }),
    );
}

fn read_modifiers() -> egui::Modifiers {
    // SAFETY: GetKeyState reads this HWND thread's message-relative keyboard state.
    // https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getkeystate
    let (ctrl, alt, shift) = unsafe {
        (
            GetKeyState(i32::from(VK_CONTROL.0)) < 0,
            GetKeyState(i32::from(VK_MENU.0)) < 0,
            GetKeyState(i32::from(VK_SHIFT.0)) < 0,
        )
    };
    egui::Modifiers {
        ctrl,
        alt,
        shift,
        command: ctrl,
        mac_cmd: false,
    }
}

fn is_pen_mouse(extra: usize) -> bool {
    extra & 0xffff_ff80 == 0xff51_5700
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_only_pen_promoted_mouse_messages() {
        for cursor in 0..128 {
            assert!(is_pen_mouse(0xff51_5700 | cursor));
            assert!(!is_pen_mouse(0xff51_5780 | cursor));
        }
        assert!(!is_pen_mouse(0));
        assert!(!is_pen_mouse(0x1234));
    }
}
