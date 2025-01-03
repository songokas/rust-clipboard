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
use std::{
    collections::HashSet,
    io::Read,
    thread::sleep,
    time::{Duration, Instant},
};
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

    // wait for target contents by polling for data but not more than 1 second
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
        let now = Instant::now();
        let initial_mime_types = mime_types()?;
        loop {
            match self.get_target_contents(target.clone(), pool_duration) {
                Ok(data) if !data.is_empty() => return Ok(data),
                Ok(_) => {
                    if initial_mime_types != mime_types()?
                        || now.elapsed() > Duration::from_millis(999)
                    {
                        return self.get_target_contents(target, pool_duration);
                    }
                    sleep(pool_duration);
                    continue;
                }
                Err(e) => return Err(e),
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

/// these tests require waylaynd with supported compositor
#[cfg(test)]
mod tests {
    use std::{collections::HashMap, process::Command, time::Duration};

    use super::*;

    type ClipboardContext = WaylandClipboardContext;

    fn get_target(target: &str) -> String {
        let output = Command::new("wl-paste")
            .args(["-t", target])
            .output()
            .expect("failed to execute xclip");
        let contents = String::from_utf8_lossy(&output.stdout);
        contents.to_string().trim_end().into()
    }

    #[serial_test::serial]
    #[test]
    fn test_get_set_contents() {
        let contents = "hello test";
        let mut context = ClipboardContext::new().unwrap();
        context.set_contents(contents.to_string()).unwrap();
        let result = context.get_contents().unwrap();
        assert_eq!(contents, result);
        assert_eq!(contents, get_target("UTF8_STRING"));
    }

    #[serial_test::serial]
    #[test]
    fn test_set_target_contents() {
        let pool_duration = Duration::from_secs(1);
        let contents = b"hello test";
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("jumbo".into(), contents.to_vec())
            .unwrap();
        let result = context
            .get_target_contents("jumbo".into(), pool_duration)
            .unwrap();
        assert_eq!(contents.to_vec(), result);
        assert_eq!(String::from_utf8_lossy(contents), get_target("jumbo"));
    }

    #[serial_test::serial]
    #[test]
    fn test_set_large_target_contents() {
        let pool_duration = Duration::from_secs(1);
        let contents = std::iter::repeat("X").take(100000).collect::<String>();
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("large".into(), contents.clone().into_bytes())
            .unwrap();
        let result = context
            .get_target_contents("large".into(), pool_duration)
            .unwrap();
        assert_eq!(contents.as_bytes().to_vec(), result);
        assert_eq!(contents, get_target("large"));
    }

    #[serial_test::serial]
    #[test]
    fn test_set_multiple_target_contents() {
        let pool_duration = Duration::from_secs(1);
        let c1 = "yes plain".as_bytes();
        let c2 = "yes html".as_bytes();
        let c3 = "yes files".as_bytes();
        let mut context = ClipboardContext::new().unwrap();
        let mut hash = HashMap::new();
        hash.insert("jumbo".into(), c1.to_vec());
        hash.insert("html".into(), c2.to_vec());
        hash.insert("files".into(), c3.to_vec());

        context.set_multiple_targets(hash).unwrap();

        let result = context
            .get_target_contents("jumbo".into(), pool_duration)
            .unwrap();
        assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
        assert_eq!(c1.to_vec(), result);

        let result = context
            .get_target_contents("html".into(), pool_duration)
            .unwrap();
        assert_eq!(c2.to_vec(), result);
        assert_eq!(String::from_utf8_lossy(c2), get_target("html".into()));

        let result = context
            .get_target_contents("files".into(), pool_duration)
            .unwrap();
        assert_eq!(c3.to_vec(), result);
        assert_eq!(String::from_utf8_lossy(c3), get_target("files"));
    }

    #[serial_test::serial]
    #[test]
    fn test_set_multiple_target_contents_with_different_contexts() {
        let pool_duration = Duration::from_millis(500);
        let c1 = "yes plain".as_bytes();
        let c2 = "yes html".as_bytes();
        let c3 = "yes files".as_bytes();
        let mut context = ClipboardContext::new().unwrap();
        let mut hash = HashMap::new();
        hash.insert("jumbo".into(), c1.to_vec());
        hash.insert("html".into(), c2.to_vec());
        hash.insert("files".into(), c3.to_vec());

        let t1 = std::thread::spawn(move || {
            context.set_multiple_targets(hash).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });

        let mut context = ClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            let result = context
                .get_target_contents("jumbo".into(), pool_duration)
                .unwrap();
            assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
            assert_eq!(c1.to_vec(), result);

            let result = context
                .get_target_contents("html".into(), pool_duration)
                .unwrap();
            assert_eq!(c2.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c2), get_target("html"));

            let result = context
                .get_target_contents("files".into(), pool_duration)
                .unwrap();
            assert_eq!(c3.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c3), get_target("files"));
            std::thread::sleep(Duration::from_millis(500));
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_wait_for_target_and_obtain_other_targets() {
        let pool_duration = Duration::from_secs(1);
        let c1 = b"yes plain";
        let c2 = b"yes html";
        let c3 = b"yes files";
        let mut context = ClipboardContext::new().unwrap();
        let mut hash = HashMap::new();
        hash.insert("jumbo".into(), c1.to_vec());
        hash.insert("html".into(), c2.to_vec());
        hash.insert("files".into(), c3.to_vec());

        let t1 = std::thread::spawn(move || {
            let result = context
                .wait_for_target_contents("jumbo".into(), pool_duration)
                .unwrap();
            assert_eq!(c1.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));

            let result = context
                .get_target_contents("html".into(), pool_duration)
                .unwrap();
            assert_eq!(c2.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c2), get_target("html"));

            let result = context
                .get_target_contents("files".into(), pool_duration)
                .unwrap();
            assert_eq!(c3.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c3), get_target("files"));
            std::thread::sleep(Duration::from_millis(500));
        });

        let mut context = ClipboardContext::new().unwrap();

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
        let c1 = b"yes files1";
        let c2 = b"yes files2";

        let mut context = ClipboardContext::new().unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context
                .wait_for_target_contents("files1".into(), pool_duration)
                .unwrap();
            assert_eq!(c1.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c1), get_target("files1"));
            let result = context
                .wait_for_target_contents("files2".into(), pool_duration)
                .unwrap();
            assert_eq!(c2.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c2), get_target("files2"));
            std::thread::sleep(Duration::from_millis(500));
        });

        let mut context = ClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            let mut hash = HashMap::new();
            hash.insert("files1".into(), c1.to_vec());
            context.set_multiple_targets(hash.clone()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            let mut hash = HashMap::new();
            hash.insert("files2".into(), c2.to_vec());
            context.set_multiple_targets(hash).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_wait_for_non_existing_target() {
        let pool_duration = Duration::from_millis(100);
        let mut context = ClipboardContext::new().unwrap();
        std::thread::spawn(move || {
            context
                .wait_for_target_contents("non-existing-target".into(), pool_duration)
                .unwrap();
            panic!("should never happen")
        });
        std::thread::sleep(Duration::from_millis(1000));
    }

    #[serial_test::serial]
    #[test]
    fn test_wait_for_non_existing_target_contents_while_changing_selection() {
        let pool_duration = Duration::from_secs(1);
        let c2 = b"yes files2";

        let mut context = ClipboardContext::new().unwrap();

        let _t1 = std::thread::spawn(move || {
            assert!(context
                .wait_for_target_contents("files1".into(), pool_duration)
                .unwrap()
                .is_empty());
            let result = context
                .wait_for_target_contents("files2".into(), pool_duration)
                .unwrap();
            assert_eq!(c2.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c2), get_target("files2"));
        });

        let mut context = ClipboardContext::new().unwrap();

        std::thread::sleep(Duration::from_millis(100));

        let t2 = std::thread::spawn(move || {
            let mut hash = HashMap::new();
            hash.insert("files2".into(), c2.to_vec());
            context.set_multiple_targets(hash.clone()).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_empty_data_returned_when_targets_change() {
        let pool_duration = Duration::from_secs(1);
        let third_target_data = b"third-target-data";
        let target = "third-target";

        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("initial-target".into(), third_target_data.to_vec())
            .unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context
                .get_target_contents(target.into(), pool_duration)
                .unwrap();
            assert!(result.is_empty());

            std::thread::sleep(Duration::from_millis(200));

            let result = context
                .get_target_contents(target.into(), pool_duration)
                .unwrap();
            assert_eq!(result, third_target_data);

            assert_eq!(
                String::from_utf8_lossy(third_target_data),
                get_target(target)
            );
            std::thread::sleep(Duration::from_millis(500));
        });
        std::thread::sleep(Duration::from_millis(100));
        let mut context = ClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            context
                .set_target_contents(target.into(), third_target_data.to_vec())
                .unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_empty_data_returned_when_multiple_targets_change() {
        let pool_duration = Duration::from_millis(50);
        let third_target_data = b"third-target-data";

        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("initial-target".into(), third_target_data.to_vec())
            .unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context
                .wait_for_target_contents("second-target".into(), pool_duration)
                .unwrap();
            assert!(result.is_empty());
        });

        let mut context = ClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            let mut hash = HashMap::new();
            hash.insert("third-target".into(), third_target_data.to_vec());
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
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("initial-target".into(), b"initial".to_vec())
            .unwrap();
        assert!(context
            .get_target_contents("second-target".into(), pool_duration)
            .unwrap()
            .is_empty());
        assert_eq!(
            context
                .get_target_contents("initial-target".into(), pool_duration)
                .unwrap(),
            b"initial"
        );
    }
}