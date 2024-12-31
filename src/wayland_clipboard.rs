/*
Copyright 2019 Gregory Meyer

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

use crate::common::*;
use core::error::Error;
use std::{collections::HashSet, io::Read, thread::sleep, time::Duration};
use wl_clipboard_rs::{
    copy::{self, MimeSource, Options, ServeRequests},
    paste, utils,
};

const MIME_URI: &str = "text/uri-list";
const MIME_BITMAP: &str = "image/png";

/// Interface to the clipboard for Wayland windowing systems.
///
/// Other users of the Wayland clipboard will only see the contents
/// copied to the clipboard so long as the process copying to the
/// clipboard exists. If you need the contents of the clipboard to
/// remain after your application shuts down, consider daemonizing the
/// clipboard components of your application.
///
/// `WaylandClipboardContext` automatically detects support for and
/// uses the primary selection protocol.
///
/// # Example
///
/// ```noop
/// use cli_clipboard::ClipboardProvider;
/// let mut clipboard = cli_clipboard::wayland_clipboard::WaylandClipboardContext::new().unwrap();
/// clipboard.set_contents("foo bar baz".to_string()).unwrap();
/// let contents = clipboard.get_contents().unwrap();
///
/// assert_eq!(contents, "foo bar baz");
/// ```
pub struct WaylandClipboardContext {
    supports_primary_selection: bool,
}

impl ClipboardProvider for WaylandClipboardContext {
    /// Constructs a new `WaylandClipboardContext` that operates on all
    /// seats using the data-control clipboard protocol.  This is
    /// intended for CLI applications that do not create Wayland
    /// windows.
    ///
    /// Attempts to detect whether the primary selection is supported.
    /// Assumes no primary selection support if no seats are available.
    /// In addition to returning Err on communication errors (such as
    /// when operating in an X11 environment), will also return Err if
    /// the compositor does not support the data-control protocol.
    fn new() -> Result<WaylandClipboardContext, Box<dyn Error>> {
        let supports_primary_selection = match utils::is_primary_selection_supported() {
            Ok(v) => v,
            Err(utils::PrimarySelectionCheckError::NoSeats) => false,
            Err(e) => return Err(e.into()),
        };

        Ok(WaylandClipboardContext {
            supports_primary_selection,
        })
    }

    /// Pastes from the Wayland clipboard.
    ///
    /// If the Wayland environment supported the primary selection when
    /// this context was constructed, first checks the primary
    /// selection. If pasting from the primary selection raises an
    /// error or the primary selection is unsupported, falls back to
    /// the regular clipboard.
    ///
    /// An empty clipboard is not considered an error, but the
    /// clipboard must indicate a text MIME type and the contained text
    /// must be valid UTF-8.
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        let data = self.get_target_contents(TargetMimeType::Text, Duration::from_millis(500))?;
        Ok(String::from_utf8(data)?)
    }

    fn get_target_contents(
        &mut self,
        target: TargetMimeType,
        _pool_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut buf = Vec::new();
        let mime_type = match &target {
            TargetMimeType::Text => paste::MimeType::Text,
            TargetMimeType::Bitmap => paste::MimeType::Specific(MIME_BITMAP),
            TargetMimeType::Files => paste::MimeType::Specific(MIME_URI),
            TargetMimeType::Specific(s) => paste::MimeType::Specific(s),
        };
        if self.supports_primary_selection {
            match paste::get_contents(
                paste::ClipboardType::Primary,
                paste::Seat::Unspecified,
                mime_type,
            ) {
                Ok((mut reader, _)) => {
                    // this looks weird, but rustc won't let me do it
                    // the natural way
                    reader.read_to_end(&mut buf).map_err(Box::new)?;
                    return Ok(buf);
                }
                Err(e) => match e {
                    paste::Error::NoSeats
                    | paste::Error::ClipboardEmpty
                    | paste::Error::NoMimeType => return Ok(Vec::new()),
                    _ => (),
                },
            }
        }

        let mut reader = match paste::get_contents(
            paste::ClipboardType::Regular,
            paste::Seat::Unspecified,
            mime_type,
        ) {
            Ok((reader, _)) => reader,
            Err(
                paste::Error::NoSeats | paste::Error::ClipboardEmpty | paste::Error::NoMimeType,
            ) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };

        reader.read_to_end(&mut buf).map_err(Box::new)?;
        Ok(buf)
    }

    /// Copies to the Wayland clipboard.
    ///
    /// If the Wayland environment supported the primary selection when
    /// this context was constructed, this will copy to both the
    /// primary selection and the regular clipboard. Otherwise, only
    /// the regular clipboard will be pasted to.
    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        self.set_target_contents(TargetMimeType::Text, data.into_bytes())
    }

    fn set_target_contents(
        &mut self,
        target: TargetMimeType,
        data: Vec<u8>,
    ) -> Result<(), Box<dyn Error>> {
        let target = get_target(target);
        let mut options = Options::new();

        options
            .seat(copy::Seat::All)
            .trim_newline(false)
            .foreground(false)
            .serve_requests(ServeRequests::Unlimited);

        if self.supports_primary_selection {
            options.clipboard(copy::ClipboardType::Both);
        } else {
            options.clipboard(copy::ClipboardType::Regular);
        }

        options
            .copy(copy::Source::Bytes(data.into()), target)
            .map_err(Into::into)
    }

    fn wait_for_target_contents(
        &mut self,
        target: TargetMimeType,
        pool_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let clipboard = if self.supports_primary_selection {
            paste::ClipboardType::Primary
        } else {
            paste::ClipboardType::Regular
        };
        let mime_types = || match paste::get_mime_types(clipboard, paste::Seat::Unspecified) {
            Ok(t) => Ok(t),
            Err(
                paste::Error::NoSeats | paste::Error::ClipboardEmpty | paste::Error::NoMimeType,
            ) => Ok(HashSet::new()),
            Err(e) => Err(e),
        };
        let initial_mime_types = mime_types()?;
        let mut initial = true;
        loop {
            let current_mime_types = if initial {
                initial = false;
                initial_mime_types.clone()
            } else {
                mime_types()?
            };
            if matches_target(&current_mime_types, &target) {
                match self.get_target_contents(target.clone(), pool_duration) {
                    Ok(data) if !data.is_empty() => return Ok(data),
                    Ok(_) => {
                        if initial_mime_types != current_mime_types {
                            return Ok(Vec::new());
                        }
                        sleep(pool_duration);
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            } else {
                if initial_mime_types != current_mime_types {
                    return Ok(Vec::new());
                }
                sleep(pool_duration);
                continue;
            }
        }
    }

    fn set_multiple_targets(
        &mut self,
        targets: impl IntoIterator<Item = (TargetMimeType, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>> {
        let targets = targets
            .into_iter()
            .map(|(k, v)| {
                let mime_type = get_target(k);
                MimeSource {
                    source: copy::Source::Bytes(v.into()),
                    mime_type,
                }
            })
            .collect();

        let mut options = Options::new();

        options
            .seat(copy::Seat::All)
            .foreground(false)
            .serve_requests(ServeRequests::Unlimited);

        if self.supports_primary_selection {
            options.clipboard(copy::ClipboardType::Both);
        } else {
            options.clipboard(copy::ClipboardType::Regular);
        }

        options.copy_multi(targets).map_err(Into::into)
    }
}

fn get_target(target: TargetMimeType) -> copy::MimeType {
    match target {
        TargetMimeType::Text => copy::MimeType::Text,
        TargetMimeType::Bitmap => copy::MimeType::Specific(MIME_BITMAP.to_string()),
        TargetMimeType::Files => copy::MimeType::Specific(MIME_URI.to_string()),
        TargetMimeType::Specific(s) => copy::MimeType::Specific(s),
    }
}

fn matches_target(types: &HashSet<String>, target: &TargetMimeType) -> bool {
    match target {
        TargetMimeType::Text => types.iter().any(|t| t.contains("text")),
        TargetMimeType::Bitmap => types.iter().any(|t| t.contains("image")),
        TargetMimeType::Files => types.iter().any(|t| t.contains(MIME_URI)),
        TargetMimeType::Specific(s) => types.iter().any(|t| t.contains(s)),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    #[ignore]
    fn wayland_test() {
        let mut clipboard =
            WaylandClipboardContext::new().expect("couldn't create a Wayland clipboard");

        clipboard
            .set_contents("foo bar baz".to_string())
            .expect("couldn't set contents of Wayland clipboard");

        assert_eq!(
            clipboard
                .get_contents()
                .expect("couldn't get contents of Wayland clipboard"),
            "foo bar baz"
        );
    }

    #[serial_test::serial]
    #[test]
    fn test_set_target_contents() {
        let pool_duration = Duration::from_secs(1);
        let contents = b"hello test";
        let mut context = WaylandClipboardContext::new().unwrap();
        context.set_target_contents("jumbo", contents).unwrap();
        let result = context.get_target_contents("jumbo", pool_duration).unwrap();
        assert_eq!(contents.to_vec(), result);
    }

    #[serial_test::serial]
    #[test]
    fn test_set_multiple_target_contents() {
        let pool_duration = Duration::from_secs(1);
        let c1 = "yes plain".as_bytes();
        let c2 = "yes html".as_bytes();
        let c3 = "yes files".as_bytes();
        let mut context = WaylandClipboardContext::new().unwrap();
        let mut hash = HashMap::new();
        hash.insert("jumbo", c1);
        hash.insert("html", c2);
        hash.insert("files", c3);

        context.set_multiple_targets(hash).unwrap();

        let result = context.get_target_contents("jumbo", pool_duration).unwrap();
        assert_eq!(c1.to_vec(), result);

        let result = context.get_target_contents("html", pool_duration).unwrap();
        assert_eq!(c2.to_vec(), result);

        let result = context.get_target_contents("files", pool_duration).unwrap();
        assert_eq!(c3.to_vec(), result);
    }

    #[serial_test::serial]
    #[test]
    fn test_wait_for_non_existing_target() {
        let pool_duration = Duration::from_secs(1);
        let mut context = WaylandClipboardContext::new().unwrap();
        std::thread::spawn(move || {
            context
                .wait_for_target_contents("non-existing-target", pool_duration)
                .unwrap();
            panic!("should never happen")
        });
        std::thread::sleep(Duration::from_millis(1500));
    }

    #[serial_test::serial]
    #[test]
    fn test_wait_for_target_and_obtain_other_targets() {
        let pool_duration = Duration::from_secs(1);
        let c1 = "yes plain".as_bytes();
        let c2 = "yes html".as_bytes();
        let c3 = "yes files".as_bytes();
        let mut context = WaylandClipboardContext::new().unwrap();
        let mut hash = HashMap::new();
        hash.insert("jumbo", c1);
        hash.insert("html", c2);
        hash.insert("files", c3);

        let t1 = std::thread::spawn(move || {
            let result = context
                .wait_for_target_contents("jumbo", pool_duration)
                .unwrap();
            // assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
            assert_eq!(c1.to_vec(), result);

            let result = context.get_target_contents("html", pool_duration).unwrap();
            assert_eq!(c2.to_vec(), result);
            // assert_eq!(String::from_utf8_lossy(c2), get_target("html"));

            let result = context.get_target_contents("files", pool_duration).unwrap();
            assert_eq!(c3.to_vec(), result);
            // assert_eq!(String::from_utf8_lossy(c3), get_target("files"));
            std::thread::sleep(Duration::from_millis(500));
        });

        let mut context = WaylandClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            context.set_multiple_targets(hash).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_wait_for_target_contents_while_changing_selection() {
        let pool_duration = Duration::from_millis(50);
        let c1 = "yes files1".as_bytes();
        let c2 = "yes files2".as_bytes();

        let mut context = WaylandClipboardContext::new().unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context
                .wait_for_target_contents("files1", pool_duration)
                .unwrap();
            assert_eq!(c1.to_vec(), result);
            let result = context
                .wait_for_target_contents("files2", pool_duration)
                .unwrap();
            assert_eq!(c2.to_vec(), result);
            std::thread::sleep(Duration::from_millis(500));
        });

        let mut context = WaylandClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            let mut hash = HashMap::new();
            hash.insert("files1", c1);
            context.set_multiple_targets(hash.clone()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            let mut hash = HashMap::new();
            hash.insert("files2", c2);
            context.set_multiple_targets(hash).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_get_target_contents_return_immediately() {
        let pool_duration = Duration::from_secs(1);
        let mut context = WaylandClipboardContext::new().unwrap();
        context
            .set_target_contents("initial-target", b"initial")
            .unwrap();
        assert!(context
            .get_target_contents("second-target", pool_duration)
            .unwrap()
            .is_empty());
        assert_eq!(
            context
                .get_target_contents("initial-target", pool_duration)
                .unwrap(),
            b"initial"
        );
    }
}
