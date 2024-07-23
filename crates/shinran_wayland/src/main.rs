use std::{
    fs::File,
    os::unix::io::AsFd,
    sync::{Arc, Mutex},
};

use wayland_client::{
    delegate_noop,
    protocol::{
        wl_display, wl_keyboard, wl_registry,
        wl_seat::{self, WlSeat},
    },
    Connection, Dispatch, QueueHandle, WEnum,
};
use wayland_protocols_misc::{
    zwp_input_method_v2::client::{
        zwp_input_method_keyboard_grab_v2::{self, ZwpInputMethodKeyboardGrabV2},
        zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
        zwp_input_method_v2::{self, ZwpInputMethodV2},
        zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2,
    },
    zwp_virtual_keyboard_v1::client::{
        zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
        zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
    },
};
use xkbcommon::xkb;

fn main() {
    let conn = Connection::connect_to_env()
        .unwrap_or_else(|_| panic!("Unable to connect to a Wayland compositor."));
    let display = conn.display();

    let mut event_queue = conn.new_event_queue::<State>();
    let qh = event_queue.handle();

    display.get_registry(&qh, ());

    let mut state = State {
        running: true,
        seats: vec![],
        configured: false,
        input_method_manager: None,
        virtual_keyboard_manager: None,
    };

    // Block until the server has received our `display.get_registry` request.
    event_queue.roundtrip(&mut state).unwrap();

    if state.input_method_manager.is_none() {
        panic!("Compositor does not support zwp_input_method_manager_v2");
    }

    // let mut event_queue = conn.new_event_queue::<Seat>();
    // let qh = event_queue.handle();
    for seat in &mut state.seats {
        seat.lock().unwrap().input_method = Some(
            state
                .input_method_manager
                .as_ref()
                .unwrap()
                .get_input_method(&seat.lock().unwrap().wl_seat, &qh, seat.clone()),
        );
    }

    while state.running {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }
}

struct State {
    input_method_manager: Option<ZwpInputMethodManagerV2>,
    virtual_keyboard_manager: Option<ZwpVirtualKeyboardManagerV1>,

    running: bool,

    seats: Vec<Arc<Mutex<Seat>>>,

    configured: bool,
}

struct Seat {
    wl_seat: WlSeat,
    input_method: Option<ZwpInputMethodV2>,
    virtual_keyboard: Option<ZwpVirtualKeyboardV1>,
    xkb_context: Option<xkb::Context>,
    xkb_keymap: Option<xkb::Keymap>,
    xkb_state: Option<xkb::State>,
    active: bool,
    enabled: bool,
    serial: u32,
    pending_activate: bool,
    pending_deactivate: bool,
    keyboard_grab: Option<ZwpInputMethodKeyboardGrabV2>,
    pressed: Vec<xkb::Keycode>,
}

unsafe impl Send for Seat {}
// unsafe impl Sync for Seat {}

impl Seat {
    fn new(wl_seat: WlSeat) -> Self {
        Self {
            wl_seat,
            input_method: None,
            virtual_keyboard: None,
            xkb_context: None,
            xkb_keymap: None,
            xkb_state: None,
            active: false,
            enabled: false,
            serial: 0,
            pending_activate: false,
            pending_deactivate: false,
            keyboard_grab: None,
            pressed: Vec::new(),
        }
    }
}

fn create_seat(state: &mut State, wl_seat: WlSeat) {
    let seat = Seat::new(wl_seat);
    state.seats.push(Arc::new(Mutex::new(seat)));
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name, interface, ..
            } => match &interface[..] {
                "wl_seat" => {
                    let seat = registry.bind::<WlSeat, _, _>(name, 1, qh, ());
                    create_seat(state, seat);
                }
                "zwp_input_method_manager_v2" => {
                    let input_man = registry.bind::<ZwpInputMethodManagerV2, _, _>(name, 1, qh, ());
                    state.input_method_manager = Some(input_man);
                }
                "zwp_virtual_keyboard_manager_v1" => {
                    let keyboard_man =
                        registry.bind::<ZwpVirtualKeyboardManagerV1, _, _>(name, 1, qh, ());
                    state.virtual_keyboard_manager = Some(keyboard_man);
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpInputMethodKeyboardGrabV2, Arc<Mutex<Seat>>> for State {
    fn event(
        state: &mut Self,
        _: &ZwpInputMethodKeyboardGrabV2,
        event: zwp_input_method_keyboard_grab_v2::Event,
        seat: &Arc<Mutex<Seat>>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_keyboard_grab_v2::Event::Key {
                time,
                key,
                state: key_state,
                ..
            } => {
                // let xkb_key = xkb::Keycode::from(key + 8);
                let WEnum::Value(key_state) = key_state else {
                    return;
                };
                seat.lock().unwrap().virtual_keyboard.as_ref().unwrap().key(
                    time,
                    key,
                    key_state.into(),
                );
            }
            zwp_input_method_keyboard_grab_v2::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                if let Some(xkb_state) = &mut seat.lock().unwrap().xkb_state {
                    // xkb_state.update_mask(
                    //     mods_depressed,
                    //     mods_latched,
                    //     mods_locked,
                    //     depressed_layout,
                    //     latched_layout,
                    //     locked_layout,
                    // );
                    seat.lock()
                        .unwrap()
                        .virtual_keyboard
                        .as_ref()
                        .unwrap()
                        .modifiers(mods_depressed, mods_latched, mods_locked, group);
                }
            }
            zwp_input_method_keyboard_grab_v2::Event::Keymap { format, fd, size } => {
                let WEnum::Value(format) = format else {
                    return;
                };
                seat.lock()
                    .unwrap()
                    .virtual_keyboard
                    .as_ref()
                    .unwrap()
                    .keymap(format.into(), fd.as_fd(), size);

                if !matches!(format, wl_keyboard::KeymapFormat::XkbV1) {
                    return;
                }
                seat.lock().unwrap().xkb_keymap = unsafe {
                    xkb::Keymap::new_from_fd(
                        seat.lock().unwrap().xkb_context.as_ref().unwrap(),
                        fd,
                        size as usize,
                        xkb::KEYMAP_FORMAT_TEXT_V1,
                        xkb::KEYMAP_COMPILE_NO_FLAGS,
                    )
                }
                .unwrap_or_else(|_| {
                    panic!("Failed to create xkb keymap from fd");
                });
                if seat.lock().unwrap().xkb_keymap.is_none() {
                    println!("Failed to compile keymap.");
                    return;
                }
                let xkb_state = xkb::State::new(seat.lock().unwrap().xkb_keymap.as_ref().unwrap());
                if xkb_state.get_raw_ptr().is_null() {
                    println!("Failed to create xkb state.");
                }
                seat.lock().unwrap().xkb_state = Some(xkb_state);
            }
            zwp_input_method_keyboard_grab_v2::Event::RepeatInfo { .. } => {}
            _ => todo!(),
        }
    }
}

impl Dispatch<ZwpInputMethodV2, Arc<Mutex<Seat>>> for State {
    fn event(
        state: &mut Self,
        input_method: &ZwpInputMethodV2,
        event: zwp_input_method_v2::Event,
        seat: &Arc<Mutex<Seat>>,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let seat_arc = seat;
        let mut seat = seat.lock().unwrap();
        match event {
            zwp_input_method_v2::Event::Activate => {
                seat.pending_activate = true;
            }
            zwp_input_method_v2::Event::Deactivate => {
                seat.pending_deactivate = true;
            }
            zwp_input_method_v2::Event::SurroundingText { .. } => {
                // Nothing.
            }
            zwp_input_method_v2::Event::TextChangeCause { .. } => {
                // Nothing.
            }
            zwp_input_method_v2::Event::ContentType { .. } => {
                // Nothing.
            }
            zwp_input_method_v2::Event::Done => {
                seat.serial += 1;
                if seat.pending_activate && !seat.active {
                    let keyboard_grab = input_method.grab_keyboard(qh, Arc::clone(seat_arc));
                    seat.keyboard_grab = Some(keyboard_grab);
                    seat.active = true;
                } else if seat.pending_deactivate && seat.active {
                    seat.keyboard_grab.as_ref().unwrap().release();
                    seat.pressed = vec![];
                    seat.keyboard_grab = None;
                    seat.active = false;
                }
                seat.pending_activate = false;
                seat.pending_deactivate = false;
            }
            zwp_input_method_v2::Event::Unavailable => {
                // Nothing.
            }
            _ => todo!(),
        }
    }
}

// Input method manager has no events.
delegate_noop!(State: ignore ZwpInputMethodManagerV2);
// Virtual keyboard manager has no events.
delegate_noop!(State: ignore ZwpVirtualKeyboardManagerV1);

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        seat: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        {
            if capabilities.contains(wl_seat::Capability::Keyboard) {
                seat.get_keyboard(qh, ());
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key { key, .. } = event {
            if key == 1 {
                // ESC key
                state.running = false;
            }
        }
    }
}
