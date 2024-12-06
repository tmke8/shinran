/// Text manipulation implementation for Shinran.
///
/// This module contains the implementation of text manipulation for the Shinran engine.
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use async_std::task::{self, sleep};
use event_listener::Event;
use ibus_utils::{
    ibus_constants, Attribute, IBusAttribute, IBusEnginePreedit, IBusText, Underline,
};
use log::{debug, info};
use xkeysym::Keysym;
use zbus::{fdo, interface, object_server::SignalContext};

use shinran_lib::Backend;

pub(crate) struct ShinranEngine {
    done: Arc<Event>,
    text: String,
    cursor_pos: u32,
    start_time: Instant,
    new_key_pressed: bool,
    backend: Arc<Backend<'static>>,
}

impl ShinranEngine {
    pub fn new(done: Arc<Event>, backend: Arc<Backend<'static>>) -> Self {
        Self {
            done,
            text: String::new(),
            cursor_pos: 0,
            start_time: Instant::now(),
            new_key_pressed: false,
            backend,
        }
    }

    async fn exit(&self) {
        info!("Exit!");
        info!("============================");
        sleep(Duration::from_millis(100)).await;
        self.done.notify(1);
    }

    async fn update_text(&self, ctxt: &SignalContext<'_>) -> zbus::Result<()> {
        debug!(
            "UpdateText(text = '{}', cursorPos = {})",
            self.text, self.cursor_pos,
        );

        let attributes = [IBusAttribute::new(
            Attribute::Underline(Underline::Single),
            0,
            text_length(&self.text),
        )];
        let ibus_text = IBusText::new(&self.text, &attributes);

        ShinranEngine::update_preedit_text(
            ctxt,
            ibus_text.into(),
            self.cursor_pos,
            !self.text.is_empty(),
            IBusEnginePreedit::Clear as u32,
        )
        .await?;

        // Spawn a task to fetch the candidates in the background.
        let backend = self.backend.clone();
        // TODO: Investigate whether this can be done without cloning the text.
        let trigger = self.text.clone();
        // `fuzzy_match` is a long-running CPU-bound operation, so we use `spawn_blocking`,
        // because we don't want to block the async runtime.
        let candidates = task::spawn_blocking(move || backend.fuzzy_match(&trigger)).await;

        if !candidates.is_empty() {
            let mut table = ibus_utils::IBusLookupTable::default();
            for (candidate, _) in candidates.into_iter().take(5) {
                table.append_candidate(candidate.0);
            }

            ShinranEngine::update_lookup_table(ctxt, table.into(), true).await?;
        }
        Ok(())
    }

    async fn clear_text(&mut self, ctxt: &SignalContext<'_>) -> zbus::Result<()> {
        self.text.clear();
        self.cursor_pos = 0;
        self.update_text(ctxt).await?;
        Ok(())
    }

    fn move_cursor(&mut self, offset: i32) {
        let text_len = self.text.len() as i32;
        self.cursor_pos = (self.cursor_pos as i32 + offset).clamp(0, text_len) as u32;
    }
}

/// Number of unicode characters in a string.
fn text_length(text: &str) -> u32 {
    text.chars().count() as u32
}

#[interface(name = "org.freedesktop.IBus.Engine")]
impl ShinranEngine {
    // ========= Method =========

    /// ProcessKeyEvent method
    async fn process_key_event(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        keyval: u32,
        keycode: u32,
        state: u32,
    ) -> fdo::Result<bool> {
        debug!(
            "ProcessKeyEvent: keyval={}, keycode={}, state={}",
            keyval, keycode, state
        );
        if state & ibus_constants::RELEASE_MASK != 0 {
            debug!("Key released");
            if !self.new_key_pressed {
                // Pass along key release events that happened before the first key press.
                ShinranEngine::forward_key_event(&ctxt, keyval, keycode, state).await?;
            }
            return Ok(true);
        } else if !self.new_key_pressed {
            self.new_key_pressed = true;
        }
        let keysym = Keysym::new(keyval);
        match keysym {
            Keysym::Return | Keysym::KP_Enter => {
                if !self.text.is_empty() {
                    let output = self.backend.check_trigger(&self.text).unwrap();
                    self.clear_text(&ctxt).await?;
                    if let Some(text) = output {
                        let ibus_text = IBusText::new(&text, &[]);
                        ShinranEngine::commit_text(&ctxt, ibus_text.into()).await?;
                    }
                }
                self.exit().await;
                return Ok(true);
            }
            Keysym::Escape => {
                self.clear_text(&ctxt).await?;
                self.exit().await;
            }
            Keysym::BackSpace => {
                if self.cursor_pos > 0 {
                    self.text.remove(self.cursor_pos as usize - 1);
                    self.cursor_pos -= 1;
                    self.update_text(&ctxt).await?;
                }
            }
            Keysym::Delete | Keysym::KP_Delete => {
                let pos = self.cursor_pos as usize;
                if pos < self.text.len() {
                    self.text.remove(pos);
                    self.update_text(&ctxt).await?;
                }
            }
            Keysym::Left | Keysym::KP_Left => {
                self.move_cursor(-1);
                self.update_text(&ctxt).await?;
            }
            Keysym::Right | Keysym::KP_Right => {
                self.move_cursor(1);
                self.update_text(&ctxt).await?;
            }
            key => {
                if let Some(character) = key.key_char() {
                    if character.is_ascii_graphic()
                        || ('\u{00A0}'..='\u{00FF}').contains(&character)
                    {
                        let pos = self.cursor_pos as usize;
                        if pos < self.text.len() {
                            self.text.insert(pos, character);
                        } else {
                            self.text.push(character);
                        }
                        self.cursor_pos += 1;
                        self.update_text(&ctxt).await?;
                    }
                }
            }
        }
        Ok(true)
    }

    /// FocusIn method
    fn focus_in(&self) {}

    /// FocusInId method
    fn focus_in_id(&self, object_path: &str, client: &str) -> fdo::Result<()> {
        info!("FocusInId: object_path={}, client={}", object_path, client);
        Ok(())
    }

    /// FocusOut method
    async fn focus_out(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> fdo::Result<()> {
        if self.start_time.elapsed() < Duration::from_millis(250) {
            info!("FocusOut was quickly after starting. Skipping exit.");
        } else {
            self.clear_text(&ctxt).await?;
            self.exit().await;
        }
        Ok(())
    }

    /// Reset method
    async fn reset(&mut self, #[zbus(signal_context)] ctxt: SignalContext<'_>) -> fdo::Result<()> {
        self.clear_text(&ctxt).await?;
        self.exit().await;
        Ok(())
    }

    /// Enable method
    fn enable(&mut self) {
        self.start_time = Instant::now();
    }

    /// Disable method
    fn disable(&self) {}

    /// Destroy method
    fn destroy(&self) {
        info!("Destroy");
        self.done.notify(1);
    }

    /// CancelHandWriting method
    fn cancel_hand_writing(&self, _n_strokes: u32) {}

    /// CandidateClicked method
    fn candidate_clicked(&self, _index: u32, _button: u32, _state: u32) {}

    /// CursorDown method
    fn cursor_down(&self) {}

    /// CursorUp method
    fn cursor_up(&self) {}

    /// FocusOutId method
    fn focus_out_id(&self, _object_path: &str) {}

    /// PageDown method
    fn page_down(&self) {}

    /// PageUp method
    fn page_up(&self) {}

    /// PanelExtensionReceived method
    fn panel_extension_received(&self, _event: zbus::zvariant::Value<'_>) {}

    /// PanelExtensionRegisterKeys method
    fn panel_extension_register_keys(&self, _data: zbus::zvariant::Value<'_>) {}

    /// ProcessHandWritingEvent method
    fn process_hand_writing_event(&self, _coordinates: Vec<f64>) {}

    /// PropertyActivate method
    fn property_activate(&self, _name: &str, _state: u32) {}

    /// PropertyHide method
    fn property_hide(&self, _name: &str) {}

    /// PropertyShow method
    fn property_show(&self, _name: &str) {}

    /// SetCapabilities method
    fn set_capabilities(&self, _caps: u32) {}

    /// SetCursorLocation method
    fn set_cursor_location(&self, _x: i32, _y: i32, _w: i32, _h: i32) {}

    /// SetSurroundingText method
    fn set_surrounding_text(
        &self,
        _text: zbus::zvariant::Value<'_>,
        _cursor_pos: u32,
        _anchor_pos: u32,
    ) {
    }

    // ========= Property =========

    /// FocusId property
    #[zbus(property)]
    fn focus_id(&self) -> bool {
        // Ok((true,))
        true
    }

    /// ActiveSurroundingText property
    #[zbus(property)]
    fn active_surrounding_text(&self) -> bool {
        false
    }

    // ========= Signals =========

    /// UpdatePreeditText signal
    #[zbus(signal)]
    async fn update_preedit_text(
        ctxt: &SignalContext<'_>,
        text: zbus::zvariant::Value<'_>,
        cursor_pos: u32,
        visible: bool,
        mode: u32,
    ) -> zbus::Result<()>;

    /// UpdateAuxiliaryText signal
    #[zbus(signal)]
    async fn update_auxiliary_text(
        ctxt: &SignalContext<'_>,
        text: zbus::zvariant::Value<'_>,
        visible: bool,
    ) -> zbus::Result<()>;

    /// CommitText signal
    #[zbus(signal)]
    async fn commit_text(
        ctxt: &SignalContext<'_>,
        text: zbus::zvariant::Value<'_>,
    ) -> zbus::Result<()>;

    /// ForwardKeyEvent signal
    #[zbus(signal)]
    async fn forward_key_event(
        ctxt: &SignalContext<'_>,
        keyval: u32,
        keycode: u32,
        state: u32,
    ) -> zbus::Result<()>;

    /// UpdateLookupTable signal
    #[zbus(signal)]
    async fn update_lookup_table(
        ctxt: &SignalContext<'_>,
        table: zbus::zvariant::Value<'_>,
        visible: bool,
    ) -> zbus::Result<()>;
}
