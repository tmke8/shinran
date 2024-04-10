/// Text manipulation implementation for Shinran.
///
/// This module contains the implementation of text manipulation for the Shinran engine.
use std::{sync::Arc, time::Duration};

use async_std::task::sleep;
use event_listener::Event;
use ibus_utils::{ibus_constants, Attribute, IBusAttribute, IBusText, Underline};
use zbus::{fdo, interface, object_server::SignalContext};

pub(crate) struct ShinranEngine {
    done: Arc<Event>,
    text: String,
    cursor_pos: u32,
}

impl ShinranEngine {
    pub fn new(done: Arc<Event>) -> Self {
        Self {
            done,
            text: String::new(),
            cursor_pos: 0,
        }
    }

    async fn update_text(&self, ctxt: &SignalContext<'_>) -> zbus::Result<()> {
        println!(
            "UpdateText(text = '{}', cursorPos = {})",
            self.text, self.cursor_pos,
        );

        let attr = IBusAttribute::new(
            Attribute::Underline(Underline::Single),
            0,
            self.text.len() as u32,
        );
        let attr_list: [IBusAttribute; 1] = [attr];
        let ibus_text = IBusText::new(&self.text, &attr_list);

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

    async fn exit(&self) {
        println!("FocusOut");
        sleep(Duration::from_millis(100)).await;
        self.done.notify(1);
    }
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
            return Ok(true);
        }
        match keyval {
            ibus_constants::KEY_ESCAPE => {
                self.clear_text(&ctxt).await?;
                self.exit().await;
            }
            ibus_constants::KEY_BACK_SPACE => {
                if self.cursor_pos > 0 {
                    self.text.remove(self.cursor_pos as usize - 1);
                    self.cursor_pos -= 1;
                    self.update_text(&ctxt).await?;
                }
                return Ok(true);
            }
            ibus_constants::KEY_DELETE | ibus_constants::KEY_KP_DELETE => {
                let pos = self.cursor_pos as usize;
                if pos < self.text.len() {
                    self.text.remove(pos);
                    self.update_text(&ctxt).await?;
                }
                return Ok(true);
            }
            _ => {
                if let Some(character) = char::from_u32(keyval) {
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
    async fn focus_out(&self) {
        self.exit().await;
    }

    /// Enable method
    fn enable(&self) {}

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

    /// CommitText signal
    #[zbus(signal)]
    async fn commit_text(
        ctxt: &SignalContext<'_>,
        text: zbus::zvariant::Value<'_>,
    ) -> zbus::Result<()>;
}
