use std::{iter::zip, rc::Rc, time::Duration};

use calloop::{
    timer::{TimeoutAction, Timer},
    Dispatcher, RegistrationToken,
};
use wayland_client::protocol::{wl_keyboard::KeyState, wl_surface::WlSurface};
use wayland_protocols_misc::{
    zwp_input_method_v2::client::{
        zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2,
        zwp_input_method_v2::ZwpInputMethodV2, zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2,
    },
    zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};
use xkbcommon::xkb::{self, Keysym};

use shinran_lib::Backend;

pub(crate) struct InputContext<S> {
    pub(crate) seat_id: u32,
    pub(crate) seat_name: Option<String>,

    input_method: ZwpInputMethodV2,
    pub(crate) virtual_keyboard: ZwpVirtualKeyboardV1,
    pub(crate) keyboard_grab: Option<ZwpInputMethodKeyboardGrabV2>,

    pub(crate) xkb_context: xkb::Context,
    pub(crate) xkb_keymap: Option<xkb::Keymap>,
    pub(crate) xkb_state: Option<xkb::State>,

    // zwp_input_method_v2
    pub(crate) pending_activate: bool,
    pub(crate) pending_deactivate: bool,
    pub(crate) num_done_events: u32, // This number is needed for the commit method.

    // zwp_input_method_keyboard_grab_v2
    // Handling repeating keys.
    pub(crate) repeat_rate: Option<Duration>,
    pub(crate) repeat_delay: Option<Duration>,
    pub(crate) pressed: [xkb::Keycode; 64],
    pub(crate) repeat_timer: Option<RepeatTimer<S>>,

    buffer: Option<String>,

    // popup
    pub(crate) wl_surface: WlSurface,
    pub(crate) popup_surface: ZwpInputPopupSurfaceV2,

    // backend
    backend: Rc<Backend<'static>>,
}

impl<S> InputContext<S> {
    pub(crate) fn new(
        seat_id: u32,
        input_method: ZwpInputMethodV2,
        virtual_keyboard: ZwpVirtualKeyboardV1,
        wl_surface: WlSurface,
        popup_surface: ZwpInputPopupSurfaceV2,
        backend: Rc<Backend<'static>>,
    ) -> Self {
        Self {
            seat_id,
            seat_name: None, // Set in `name` event in WlSeat.
            input_method,
            virtual_keyboard,
            xkb_context: xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
            xkb_keymap: None, // Set in `keymap` event.
            xkb_state: None,  // Set in `keymap` event.
            num_done_events: 0,
            pending_activate: false,
            pending_deactivate: false,
            keyboard_grab: None, // Set in `done` event in InputMethod.
            repeat_rate: None,   // Set in `repeat_info` event.
            repeat_delay: None,  // Set in `repeat_info` event.
            pressed: [xkb::Keycode::default(); 64],
            repeat_timer: None, // Set as needed.
            wl_surface,
            popup_surface,
            buffer: None, // Set as needed.
            backend,
        }
    }

    fn append(&mut self, ch: char) {
        if let Some(ref mut preedit_str) = self.buffer {
            preedit_str.push(ch);
        } else {
            self.buffer = Some(ch.to_string());
        }
    }

    pub(crate) fn mark_as_pressed(&mut self, keycode: xkb::Keycode) {
        for code in self.pressed.iter_mut() {
            // Find an empty slot and store the keycode.
            // With 64 slots, there should always be an empty one.
            if *code == xkb::Keycode::default() {
                *code = keycode;
                break;
            }
        }
        eprintln!("Added key!");
        let mut pressed_num = [0u32; 64];
        for (num, code) in zip(pressed_num.iter_mut(), self.pressed) {
            *num = code.into();
        }
        eprintln!("{:?}", pressed_num);
    }

    pub(crate) fn release_if_pressed(&mut self, keycode: xkb::Keycode) -> bool {
        for code in self.pressed.iter_mut() {
            if *code == keycode {
                // Clear the slot.
                *code = xkb::Keycode::default();
                eprintln!("Removed key!");
                let mut pressed_num = [0u32; 64];
                for (num, code) in zip(pressed_num.iter_mut(), self.pressed) {
                    *num = code.into();
                }
                eprintln!("{:?}", pressed_num);
                return true;
            }
        }
        false
    }

    /// Returns None if the program should wind down.
    pub(crate) fn handle_key(&mut self, xkb_key: xkb::Keycode) -> Option<bool> {
        let handled: Option<bool>;
        let xkb_state = self.xkb_state.as_ref().unwrap();
        let sym = xkb_state.key_get_one_sym(xkb_key);
        match sym {
            Keysym::Escape => {
                // Commit an empty string.
                self.composing_update(String::default());
                // return Some(true);
                return None; // shutdown
            }
            Keysym::Return => {
                // Send the text.
                if let Some(buffer) = &mut self.buffer {
                    let output = self.backend.check_trigger(buffer).unwrap();
                    if let Some(output) = output {
                        // found match
                        self.composing_commit(output);
                    } else {
                        self.composing_update(String::default());
                    }
                    self.buffer = None;
                }
                // return Some(true);
                return None; // shutdown
            }
            Keysym::KP_Space | Keysym::space => {
                return Some(false);
            }
            _ => {
                // If the key corresponds to an ASCII character, add it to the buffer.
                // Otherwise, mark it as unhandled.
                if let Some(ch) = char::from_u32(xkb_state.key_get_utf32(xkb_key)) {
                    if ch.is_ascii() {
                        if ch == '\0' {
                            // If the key does not represent a character,
                            // `key_get_utf32` returns 0.
                            handled = Some(false);
                        } else {
                            self.append(ch);
                            handled = Some(true);
                        }
                    } else {
                        handled = Some(false);
                    }
                } else {
                    handled = Some(false);
                }
            }
        }
        if let Some(text) = &self.buffer {
            // TODO: Only update if the text has changed.
            self.composing_update(text.clone());
        }
        handled
    }

    pub(crate) fn repeat_key(&mut self) -> TimeoutAction {
        let repeat_rate = self.repeat_rate.expect("Repeat rate should have been set.");
        let repeating = self
            .repeat_timer
            .as_mut()
            .expect("Repeat timer should have been set.");
        let key_code = repeating.keycode;
        let key = u32::from(key_code) - 8;
        eprintln!("Timer repeats {}", key);
        let time = repeating.timestamp;
        // Update the timestamp for the next repetition.
        repeating.timestamp += 1000 / (repeat_rate.as_millis() as u32);
        if self.handle_key(key_code).is_some_and(|x| !x) {
            self.virtual_keyboard
                .key(time, key, KeyState::Pressed.into());
            self.repeat_timer = None;
            // Don't schedule the timer again.
            eprintln!("Timer dropped.");
            return TimeoutAction::Drop;
        }
        TimeoutAction::ToDuration(repeat_rate)
    }

    fn composing_update(&mut self, text: String) {
        let cursor_end = text.chars().count() as i32;
        self.input_method
            .set_preedit_string(text, cursor_end, cursor_end);
        self.input_method.commit(self.num_done_events);
    }

    fn composing_commit(&mut self, output: String) {
        self.input_method.commit_string(output);
        self.input_method.commit(self.num_done_events);
    }

    fn draw_popup(&mut self) {
        todo!("Draw popup!");
    }
}

pub(crate) struct RepeatTimer<S> {
    pub(crate) keycode: xkb::Keycode,
    pub(crate) timestamp: u32,
    pub(crate) timer: Dispatcher<'static, Timer, S>,
    pub(crate) registration: RegistrationToken,
}
