//! Content dragged over a window from another application.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use smithay_client_toolkit::data_device_manager::data_offer::DragOffer;
use zgui_geom::{Css, CssPx, Point};
use zgui_platform::{DragEvent, SurfaceId};
use zgui_vocab::{DragData, SharedString};

/// The drag that is currently over a window, if there is one.
///
/// # Why the content is read before the drop
///
/// The contract carries everything dragged at every stage, including the one where the pointer is
/// still moving, so that a drop target can decide whether it *would* accept before the user lets
/// go — which is what makes a highlight appear while the drag is happening rather than after.
///
/// The protocol does not hand the content over: it names the media types on offer and leaves each
/// to be asked for through a pipe. So the reads start when the drag arrives, one per form the
/// offer has: the address list, plain text and markup, all at once, because a source may write
/// them in any order. The `Entered` event is produced when all of them finish. A drag whose reads have not finished by the time the user drops is still
/// delivered — the drop waits for the same reads rather than arriving with nothing.
///
/// # What a drop target gets
///
/// * [`DragEvent::Entered`] with everything dragged, when the reads finish before the drop. It has
///   the latest pointer position.
/// * [`DragEvent::Moved`] for each pointer motion after `Entered`. Motion before it is not sent.
/// * [`DragEvent::Left`] when the drag leaves after `Entered`.
/// * [`DragEvent::Dropped`] with everything dragged, when the user lets go and the reads are
///   finished. A drop before the reads finish sends `Dropped` without an `Entered` before it.
#[derive(Debug, Default)]
pub struct Drag {
    /// The session, while one is over a window.
    session: Option<Session>,
    /// Where a completed read leaves its answer.
    read: Arc<Mutex<Option<DragData>>>,
}

/// One drag over one window.
#[derive(Debug)]
struct Session {
    /// The window it is over.
    surface: SurfaceId,
    /// The offer the content is asked for through.
    offer: DragOffer,
    /// Where the pointer is.
    at: Point<CssPx, Css>,
    /// Whether the content has been reported yet.
    announced: bool,
    /// Whether the user has let go and the drop is waiting on the read.
    dropped: bool,
}

impl Drag {
    /// Records a drag arriving over `surface`.
    pub fn entered(&mut self, surface: SurfaceId, offer: DragOffer, at: Point<CssPx, Css>) {
        *self.read.lock().unwrap_or_else(|held| held.into_inner()) = None;
        self.session = Some(Session {
            surface,
            offer,
            at,
            announced: false,
            dropped: false,
        });
    }

    /// The offer to read the content out of, when one has just arrived.
    pub fn to_read(&self) -> Option<&DragOffer> {
        let session = self.session.as_ref()?;
        (!session.announced).then_some(&session.offer)
    }

    /// Where a completed read leaves its answer.
    pub fn answers(&self) -> Arc<Mutex<Option<DragData>>> {
        Arc::clone(&self.read)
    }

    /// Records the pointer moving.
    pub fn moved(&mut self, at: Point<CssPx, Css>) -> Option<(SurfaceId, DragEvent)> {
        let session = self.session.as_mut()?;
        session.at = at;
        session
            .announced
            .then_some((session.surface, DragEvent::Moved { position: at }))
    }

    /// Records the drag leaving without a drop.
    pub fn left(&mut self) -> Option<(SurfaceId, DragEvent)> {
        let session = self.session.take()?;
        session
            .announced
            .then_some((session.surface, DragEvent::Left))
    }

    /// Records the user letting go.
    ///
    /// Answers with the drop when the content is already known, and with nothing when it is not
    /// — in which case the reads that are still running produce it.
    pub fn dropped(&mut self) -> Option<(SurfaceId, DragEvent)> {
        let session = self.session.as_mut()?;
        session.dropped = true;
        let data = self
            .read
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone()?;
        Some(self.finish(data))
    }

    /// Records the reads finishing, and says what that means now.
    ///
    /// Before the drop it is the arrival; after it, it is the drop itself. Either way it is the
    /// first moment the content is known, which is the only moment either event can be produced.
    pub fn read_finished(&mut self, data: DragData) -> Option<(SurfaceId, DragEvent)> {
        *self.read.lock().unwrap_or_else(|held| held.into_inner()) = Some(data.clone());
        let session = self.session.as_mut()?;
        if session.dropped {
            return Some(self.finish(data));
        }
        if session.announced {
            return None;
        }
        session.announced = true;
        Some((
            session.surface,
            DragEvent::Entered {
                data,
                position: session.at,
            },
        ))
    }

    /// Completes the drop and takes the session down.
    fn finish(&mut self, data: DragData) -> (SurfaceId, DragEvent) {
        let session = self
            .session
            .take()
            .expect("a drop is only finished while a session exists");
        // The offer is told the transfer worked and then retired. A drop that is never finished
        // leaves the source waiting, which on some desktops leaves the dragged icon on screen.
        session.offer.finish();
        session.offer.destroy();
        (
            session.surface,
            DragEvent::Dropped {
                data,
                position: session.at,
            },
        )
    }
}

/// One form of dragged content this backend reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// The address list, which holds the files and the other addresses.
    Addresses,
    /// Plain text.
    Text,
    /// Markup.
    Html,
}

/// The forms an offer has that this backend reads, each with the media type to ask for it in,
/// spelled as the offer spells it.
///
/// The address list comes first: it is the form a file drop needs, so it is the one accepted.
pub fn wanted(offered: &[String]) -> Vec<(Form, String)> {
    use crate::clipboard::mime;

    [
        (Form::Addresses, mime::spelled(offered, mime::URI_LIST)),
        (Form::Text, mime::named_text(offered)),
        (Form::Html, mime::spelled(offered, mime::HTML)),
    ]
    .into_iter()
    .filter_map(|(form, mime)| mime.map(|mime| (form, mime)))
    .collect()
}

/// Puts what was read in `form` into `data`.
///
/// Text is decoded as the clipboard decodes it: as UTF-8, with anything that is not replaced.
pub fn fill(data: &mut DragData, form: Form, bytes: &[u8]) {
    match form {
        Form::Addresses => {
            let (paths, uris) = addresses(bytes);
            data.paths = paths;
            data.uris = uris;
        }
        Form::Text => data.text = Some(SharedString::from(String::from_utf8_lossy(bytes).as_ref())),
        Form::Html => data.html = Some(SharedString::from(String::from_utf8_lossy(bytes).as_ref())),
    }
}

/// The local files and the other addresses a list of addresses names, each in list order.
///
/// The format is one address per line with comment lines. Only a `file` address on this machine
/// is a path: one with no host or the host `localhost`. Every other address, a web address or a
/// file on another host, is kept as written, because turning it into a path would produce a name
/// nothing can open.
pub fn addresses(bytes: &[u8]) -> (Vec<PathBuf>, Vec<SharedString>) {
    let mut paths = Vec::new();
    let mut uris = Vec::new();
    for line in String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        match local_file(line) {
            Some(address) => paths.push(PathBuf::from(unescape(address))),
            None => uris.push(SharedString::from(line)),
        }
    }
    (paths, uris)
}

/// The escaped path of a `file` address on this machine.
fn local_file(address: &str) -> Option<&str> {
    let rest = address.strip_prefix("file://")?;
    if rest.starts_with('/') {
        return Some(rest);
    }
    rest.strip_prefix("localhost")
        .filter(|path| path.starts_with('/'))
}

/// One address with its percent escapes resolved.
fn unescape(address: &str) -> String {
    let mut out = String::with_capacity(address.len());
    let bytes = address.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let pair = &address[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(pair, 16) {
                out.push(byte as char);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Form, addresses, fill, wanted};
    use std::path::PathBuf;
    use zgui_vocab::{DragData, SharedString};

    fn paths(bytes: &[u8]) -> Vec<PathBuf> {
        addresses(bytes).0
    }

    #[test]
    fn a_list_with_nothing_on_it_names_no_files() {
        assert_eq!(addresses(b""), (Vec::new(), Vec::new()));
        assert_eq!(
            addresses(b"# a comment and nothing else\r\n"),
            (Vec::new(), Vec::new())
        );
    }

    #[test]
    fn every_local_file_address_on_the_list_becomes_a_path() {
        let list = b"file:///home/a/one.png\r\nfile://localhost/home/a/two.png\r\n";
        assert_eq!(
            paths(list),
            vec![
                PathBuf::from("/home/a/one.png"),
                PathBuf::from("/home/a/two.png")
            ]
        );
    }

    #[test]
    fn an_address_that_names_no_local_file_is_kept_as_written() {
        // A web address or a file on another host made into a path is a name nothing can open.
        let mixed =
            b"https://example.invalid/a\r\nfile:///home/a/one.png\r\nfile://server/share/b\r\n";
        assert_eq!(
            addresses(mixed),
            (
                vec![PathBuf::from("/home/a/one.png")],
                vec![
                    SharedString::from("https://example.invalid/a"),
                    SharedString::from("file://server/share/b"),
                ],
            )
        );
    }

    #[test]
    fn an_escaped_name_comes_back_as_the_name_it_stands_for() {
        let escaped = b"file:///home/a/two%20words%21.txt\r\n";
        assert_eq!(
            paths(escaped),
            vec![PathBuf::from("/home/a/two words!.txt")]
        );
    }

    #[test]
    fn an_escape_that_is_not_one_is_left_alone_rather_than_dropping_the_character() {
        let odd = b"file:///home/a/100%.txt\r\n";
        assert_eq!(paths(odd), vec![PathBuf::from("/home/a/100%.txt")]);
    }

    #[test]
    fn each_form_an_offer_has_is_asked_for_with_the_address_list_first() {
        let offered: Vec<String> = ["text/html", "text/plain;charset=utf-8", "text/uri-list"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(
            wanted(&offered),
            vec![
                (Form::Addresses, "text/uri-list".to_owned()),
                (Form::Text, "text/plain;charset=utf-8".to_owned()),
                (Form::Html, "text/html".to_owned()),
            ]
        );
        assert!(wanted(&["image/png".to_owned()]).is_empty());
    }

    #[test]
    fn a_link_dragged_from_a_browser_arrives_as_an_address_and_as_text() {
        let mut data = DragData::default();
        fill(&mut data, Form::Addresses, b"https://example.invalid/a\r\n");
        fill(&mut data, Form::Text, b"https://example.invalid/a");
        assert!(data.paths.is_empty());
        assert_eq!(
            data.uris,
            vec![SharedString::from("https://example.invalid/a")]
        );
        assert_eq!(data.text.as_deref(), Some("https://example.invalid/a"));
    }
}
