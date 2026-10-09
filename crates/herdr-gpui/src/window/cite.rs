//! The pane menu's "Cite Selection": the highlighted text goes into the
//! pane's prompt as a quotation, never submitted.

use super::HerdrWindow;
use crate::{Error, Result, connection::ConnectionBridge, terminal::InputTarget};
use gpui::{Context, Window};
use herdr_client::protocol::ClientPaneInputEvent;

const MAX_CITE_BYTES: usize = 64 * 1024;

impl HerdrWindow {
    pub(crate) fn cite_into_pane(
        &mut self,
        pane_id: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = quote_text(text).and_then(|quoted| {
            let handle = self.endpoints[self.selected_endpoint]
                .connection
                .handle
                .as_ref()
                .ok_or(Error::NotConnected)?;
            let snapshot = self.live.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
            ConnectionBridge::send_input(
                handle,
                &snapshot.boot_id,
                &InputTarget::Pane(pane_id.to_owned()),
                ClientPaneInputEvent::Paste(quoted),
            )?;
            Ok(())
        });
        match result {
            Ok(()) => window.focus(&self.focus, cx),
            Err(error) => {
                self.local_error = Some(format!("Text not cited: {error}"));
                cx.notify();
            }
        }
    }
}

/// Each line prefixed with `> `, trailing blanks trimmed, and no trailing
/// newline so the quotation waits in the prompt for the user's reply.
pub(super) fn quote_text(text: &str) -> Result<String> {
    if text.len() > MAX_CITE_BYTES {
        return Err(Error::CiteSize);
    }
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let end = lines
        .iter()
        .rposition(|line| !line.is_empty())
        .map_or(0, |index| index + 1);
    let quoted = lines[..end]
        .iter()
        .map(|line| {
            let clean: String = line.chars().filter(|ch| !ch.is_control()).collect();
            if clean.is_empty() {
                ">".to_owned()
            } else {
                format!("> {clean}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if quoted.is_empty() || quoted.len() > MAX_CITE_BYTES {
        return Err(Error::CiteSize);
    }
    Ok(quoted)
}

#[cfg(test)]
mod tests {
    use super::quote_text;

    #[test]
    fn quotes_each_line_and_trims_trailing_blanks() -> crate::Result<()> {
        assert_eq!(quote_text("a  \n\nb\n\n")?, "> a\n>\n> b");
        Ok(())
    }

    #[test]
    fn strips_control_characters_and_rejects_empty() -> crate::Result<()> {
        assert_eq!(quote_text("x\u{1b}[31my")?, "> x[31my");
        assert!(quote_text("\n \n").is_err());
        Ok(())
    }

    #[test]
    fn rejects_oversized_text() {
        assert!(quote_text(&"a".repeat(64 * 1024 + 1)).is_err());
    }
}
