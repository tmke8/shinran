/// Text manipulation implementation for Shinran.
///
/// This module contains the implementation of text manipulation for the Shinran engine.
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use async_std::task::sleep;
use event_listener::Event;
use ibus_utils::{ibus_constants, Attribute, IBusAttribute, IBusText, Underline};
use xkeysym::Keysym;
use zbus::{fdo, interface, object_server::SignalContext};

use shinran_lib::Backend;

pub(crate) struct ShinranEngine {
    done: Arc<Event>,
    text: String,
    cursor_pos: u32,
    start_time: Instant,
    new_key_pressed: bool,
    backend: Backend,
}

impl ShinranEngine {
    pub fn new(done: Arc<Event>) -> Self {
        let cli_overrides = HashMap::new();
        Self {
            done,
            text: String::new(),
            cursor_pos: 0,
            start_time: Instant::now(),
            new_key_pressed: false,
            backend: Backend::new(&cli_overrides).unwrap(),
        }
    }

    async fn exit(&self) {
        println!("FocusOut");
        sleep(Duration::from_millis(100)).await;
        self.done.notify(1);
    }

    async fn update_text(&self, ctxt: &SignalContext<'_>) -> zbus::Result<()> {
        println!(
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
        )
        .await?;
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
    /// ProcessKeyEvent method
    async fn process_key_event(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        keyval: u32,
        keycode: u32,
        state: u32,
    ) -> fdo::Result<bool> {
        println!(
            "ProcessKeyEvent: keyval={}, keycode={}, state={}",
            keyval, keycode, state
        );
        if state & ibus_constants::RELEASE_MASK != 0 {
            println!("Key released");
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

    /// FocusOut method
    async fn focus_out(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
    ) -> fdo::Result<()> {
        if self.start_time.elapsed() < Duration::from_millis(250) {
            eprintln!("FocusOut was quickly after starting. Skipping exit.");
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
        println!("Destroy");
        self.done.notify(1);
    }

    /// UpdatePreeditText signal
    #[zbus(signal)]
    async fn update_preedit_text(
        ctxt: &SignalContext<'_>,
        text: zbus::zvariant::Value<'_>,
        cursor_pos: u32,
        visible: bool,
    ) -> zbus::Result<()>;

    /// UpdatePreeditTextWithMode signal
    #[zbus(signal)]
    async fn update_preedit_text_with_mode(
        ctxt: &SignalContext<'_>,
        text: zbus::zvariant::Value<'_>,
        cursor_pos: u32,
        visible: bool,
        mode: u32,
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
}
