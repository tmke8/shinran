/// Text manipulation implementation for Shinran.
///
/// This module contains the implementation of text manipulation for the Shinran engine.
use ibus_utils::{ibus_text, Attribute, IBUS_ATTR_TYPE_UNDERLINE, IBUS_ATTR_UNDERLINE_SINGLE};
use zbus::object_server::SignalContext;

use super::ShinranEngine;

impl ShinranEngine {
    pub async fn update_text(&self, ctxt: SignalContext<'_>) -> zbus::Result<()> {
        println!(
            "UpdateText(text = '{}', cursorPos = {})",
            self.text, self.cursor_pos,
        );

        let attr = Attribute {
            type_: IBUS_ATTR_TYPE_UNDERLINE,
            value: IBUS_ATTR_UNDERLINE_SINGLE,
            start_index: 0,
            end_index: self.text.len() as u32,
        };
        let attr_list: [Attribute; 1] = [attr];
        let ibus_text = ibus_text(&self.text, &attr_list);

        ShinranEngine::update_preedit_text(&ctxt, ibus_text, self.cursor_pos, self.text != "")
            .await?;
        Ok(())
    }

    pub async fn clear_text(&mut self, ctxt: SignalContext<'_>) -> zbus::Result<()> {
        self.text.clear();
        self.cursor_pos = 0;
        self.update_text(ctxt).await?;
        Ok(())
    }
}
