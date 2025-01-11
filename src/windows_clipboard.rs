/*
Copyright 2016 Avraham Weinstock

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

use clipboard_win::empty;
use clipboard_win::formats::Bitmap;
use clipboard_win::formats::FileList;
use clipboard_win::formats::RawData;
use clipboard_win::get_clipboard;
use clipboard_win::options::NoClear;
use clipboard_win::raw::set_bitmap_with;
use clipboard_win::raw::set_file_list;
use clipboard_win::raw::set_string_with;
use clipboard_win::raw::set_without_clear;
use clipboard_win::Clipboard;
use clipboard_win::EnumFormats;
use clipboard_win::SysResult;
use clipboard_win::{get_clipboard_string, set_clipboard_string};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::common::TargetMimeType;
use crate::ClipboardProvider;
use std::error::Error;

const RETRY_ATTEMPS: usize = 10;
const UNEXPECTED_ITEM_CODE: i32 = 1168;
const MAX_WAIT_DURATION: Duration = Duration::from_millis(999);

// prevent heap corruption errors or attemps to obtain clipboard failures
static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub struct WindowsClipboardContext;

impl ClipboardProvider for WindowsClipboardContext {
    fn new() -> Result<Self, Box<dyn Error>> {
        Ok(WindowsClipboardContext)
    }
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        let _l = LOCK.lock().expect("Win clipboard lock");
        Ok(get_clipboard_string()?)
    }
    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        let _l = LOCK.lock().expect("Win clipboard lock");
        Ok(set_clipboard_string(&data)?)
    }

    fn get_target_contents(
        &mut self,
        target: TargetMimeType,
        _poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let handle_result = |result: SysResult<_>| match result {
            Ok(d) => Ok(d),
            Err(code) if matches!(code.raw_code(), UNEXPECTED_ITEM_CODE) => Ok(Vec::new()),
            Err(e) => Err(e),
        };
        Ok(match target {
            TargetMimeType::Text => self.get_contents().map(|s| s.into_bytes())?,
            TargetMimeType::Bitmap => {
                let _l = LOCK.lock().expect("Win clipboard lock");
                handle_result(get_clipboard(Bitmap))?
            }
            TargetMimeType::Files => {
                let _l = LOCK.lock().expect("Win clipboard lock");
                handle_result(
                    get_clipboard(FileList).map(|list: Vec<String>| list.join("\n").into_bytes()),
                )?
            }
            TargetMimeType::Specific(s) => {
                let format_id: u32 = s.parse()?;
                let _l = LOCK.lock().expect("Win clipboard lock");
                handle_result(get_clipboard(RawData(format_id)))?
            }
        })
    }

    // wait for target contents by polling for data but not more than 1 second
    fn wait_for_target_contents(
        &mut self,
        target: TargetMimeType,
        poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let initial_formats = self.list_targets()?;
        let now = Instant::now();
        loop {
            match self.get_target_contents(target.clone(), poll_duration) {
                Ok(data) if !data.is_empty() => return Ok(data),
                Ok(_) => {
                    if initial_formats != self.list_targets()? || now.elapsed() > MAX_WAIT_DURATION
                    {
                        return self.get_target_contents(target, poll_duration);
                    }
                    sleep(poll_duration);
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn set_target_contents(
        &mut self,
        target: TargetMimeType,
        data: Vec<u8>,
    ) -> Result<(), Box<dyn Error>> {
        self.clear()?;
        set_target_contents(target, data)
    }

    fn set_multiple_targets(
        &mut self,
        targets: impl IntoIterator<Item = (TargetMimeType, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>> {
        self.clear()?;
        for (key, value) in targets {
            set_target_contents(key, value)?;
        }
        Ok(())
    }

    fn list_targets(&self) -> Result<Vec<TargetMimeType>, Box<dyn Error>> {
        let _l = LOCK.lock().expect("Win clipboard lock");
        let _clip = Clipboard::new_attempts(RETRY_ATTEMPS)?;
        Ok(EnumFormats::new()
            .into_iter()
            .map(|s| TargetMimeType::Specific(s.to_string()))
            .collect())
    }

    fn clear(&mut self) -> Result<(), Box<dyn Error>> {
        let _l = LOCK.lock().expect("Win clipboard lock");
        let _clip = Clipboard::new_attempts(RETRY_ATTEMPS)?;
        empty().map_err(Into::into)
    }
}

fn set_target_contents(target: TargetMimeType, data: Vec<u8>) -> Result<(), Box<dyn Error>> {
    let _l = LOCK.lock().expect("Win clipboard lock");
    let _clip = Clipboard::new_attempts(RETRY_ATTEMPS)?;
    Ok(match target {
        TargetMimeType::Text => set_string_with(std::str::from_utf8(&data)?, NoClear)?,
        TargetMimeType::Bitmap => set_bitmap_with(&data, NoClear)?,
        TargetMimeType::Files => {
            let content = String::from_utf8(data)?;
            let files: Vec<&str> = content.lines().collect();
            set_file_list(&files)?
        }
        TargetMimeType::Specific(s) => {
            let format_id: u32 = s.parse()?;
            set_without_clear(format_id, &data)?
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, path::absolute, process::Command};

    const MIME_TEXT: &str = "1";
    const MIME_CUSTOM1: &str = "768";
    const MIME_CUSTOM2: &str = "769";
    const MIME_CUSTOM3: &str = "770";

    type ClipboardContext = WindowsClipboardContext;

    fn get_target() -> String {
        let output = Command::new("powershell")
            .args(&["-command", "Get-Clipboard", "-Raw"])
            .output()
            .expect("failed to execute powershell");
        let contents = String::from_utf8_lossy(&output.stdout);
        contents.trim_end().into()
    }

    #[serial_test::serial]
    #[test]
    fn test_set_get_contents() {
        let contents = "hello test";
        let mut context = ClipboardContext::new().unwrap();
        context.set_contents(contents.to_string()).unwrap();
        let result = context.get_contents().unwrap();
        assert_eq!(contents, result);
        assert_eq!(contents, get_target());
    }

    #[serial_test::serial]
    #[test]
    fn test_list_targets() {
        let contents = "hello test";
        let mut context = ClipboardContext::new().unwrap();
        context.set_contents(contents.to_string()).unwrap();
        let targets = context
            .list_targets()
            .unwrap()
            .into_iter()
            .map(|t| match t {
                TargetMimeType::Specific(s) => s,
                _ => panic!("unexpected"),
            })
            .collect::<Vec<String>>()
            .join("\n");
        assert_eq!(targets, "13\n16\n1\n7");
    }

    #[serial_test::serial]
    #[test]
    fn test_set_get_defined_targets() {
        let poll_duration = Duration::from_secs(1);
        let full_path1 = absolute("osx_clipboard.rs").unwrap();
        let full_path2 = absolute("x11_clipboard.rs").unwrap();
        let expected_bmp: [u8; 58] = [
            66, 77, 58, 0, 0, 0, 0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1,
            0, 32, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255,
            255, 255, 0,
        ];
        let data = [
            (
                TargetMimeType::Text,
                b"any text".to_vec(),
                b"any text".to_vec(),
            ),
            (
                TargetMimeType::Files,
                format!(
                    "{}\n{}",
                    full_path1.to_string_lossy(),
                    full_path2.to_string_lossy()
                )
                .into_bytes(),
                format!(
                    "{}\n{}",
                    full_path1.to_string_lossy(),
                    full_path2.to_string_lossy()
                )
                .into_bytes(),
            ),
            (
                TargetMimeType::Bitmap,
                BMP_DATA.to_vec(),
                expected_bmp.to_vec(),
            ),
            (
                TargetMimeType::Specific(MIME_TEXT.to_string()),
                b"x-clipsync\0".to_vec(),
                b"x-clipsync\0".to_vec(),
            ),
        ];
        for (target, contents, expected) in data {
            let mut context = ClipboardContext::new().unwrap();
            context
                .set_target_contents(target.clone(), contents.clone())
                .unwrap();
            let result = context.get_target_contents(target, poll_duration).unwrap();
            assert_eq!(expected, result); //, "{}", std::str::from_utf8(&result).unwrap());
        }
    }

    #[serial_test::serial]
    #[test]
    fn test_set_target_contents() {
        let poll_duration = Duration::from_secs(1);
        let contents = b"hello test";
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents(MIME_CUSTOM1.into(), contents.to_vec())
            .unwrap();
        let result = context
            .get_target_contents(MIME_CUSTOM1.into(), poll_duration)
            .unwrap();
        assert_eq!(contents.as_slice(), result);
    }

    #[serial_test::serial]
    #[test]
    fn test_set_large_target_contents() {
        let poll_duration = Duration::from_secs(1);
        let mut contents = std::iter::repeat("X").take(100000).collect::<String>();
        contents.push_str("\0");
        let mut context = ClipboardContext::new().unwrap();
        context.clear().unwrap();

        context
            .set_target_contents(MIME_TEXT.into(), contents.clone().into_bytes())
            .unwrap();
        let result = context
            .get_target_contents(MIME_TEXT.into(), poll_duration)
            .unwrap();
        assert_eq!(contents.len(), result.len());
        assert_eq!(contents.as_bytes(), result);
        assert_eq!(contents.trim_end_matches(char::from(0)), get_target());
    }

    #[serial_test::serial]
    #[test]
    fn test_set_multiple_target_contents() {
        let poll_duration = Duration::from_secs(1);
        let c1 = "yes plain\0";
        let c2 = "yes html";
        let c3 = "yes files";
        let mut context = ClipboardContext::new().unwrap();
        let mut hash = Vec::new();
        hash.push((MIME_CUSTOM1.into(), c2.as_bytes().to_vec()));
        hash.push((MIME_CUSTOM2.into(), c3.as_bytes().to_vec()));
        hash.push((MIME_TEXT.into(), c1.as_bytes().to_vec()));
        context.set_multiple_targets(hash).unwrap();

        let result = context
            .get_target_contents(MIME_TEXT.into(), poll_duration)
            .unwrap();
        assert_eq!(c1.as_bytes(), result);

        let result = context
            .get_target_contents(MIME_CUSTOM1.into(), poll_duration)
            .unwrap();
        assert_eq!(c2.as_bytes(), result);

        let result = context
            .get_target_contents(MIME_CUSTOM2.into(), poll_duration)
            .unwrap();
        assert_eq!(c3.as_bytes(), result);
    }

    #[serial_test::serial]
    #[test]
    fn test_set_multiple_target_contents_with_different_contexts() {
        let poll_duration = Duration::from_millis(500);
        let c1 = "yes plain".as_bytes();
        let c2 = "yes html".as_bytes();
        let mut context = ClipboardContext::new().unwrap();
        let mut hash = Vec::new();
        hash.push((MIME_CUSTOM1.into(), c1.to_vec()));
        hash.push((MIME_CUSTOM2.into(), c2.to_vec()));

        let t1 = std::thread::spawn(move || {
            context.set_multiple_targets(hash).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });

        std::thread::sleep(Duration::from_millis(100));

        let mut context = ClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            let result = context
                .get_target_contents(MIME_CUSTOM1.into(), poll_duration)
                .unwrap();
            assert_eq!(c1.to_vec(), result);

            let result = context
                .get_target_contents(MIME_CUSTOM2.into(), poll_duration)
                .unwrap();
            assert_eq!(c2.to_vec(), result);
            std::thread::sleep(Duration::from_millis(500));
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_wait_for_target_and_obtain_other_targets() {
        let poll_duration = Duration::from_secs(1);
        let c1 = b"yes plain";
        let c2 = b"yes html";
        // let c3 = b"yes files";
        let mut context = ClipboardContext::new().unwrap();
        let mut hash = Vec::new();
        hash.push((MIME_CUSTOM1.into(), c1.to_vec()));
        hash.push((MIME_CUSTOM2.into(), c2.to_vec()));
        context.set_multiple_targets(Vec::new()).unwrap();
        let t1 = std::thread::spawn(move || {
            let result = context
                .wait_for_target_contents(MIME_CUSTOM1.into(), poll_duration)
                .unwrap();
            assert_eq!(c1.as_slice(), result);

            let result = context
                .get_target_contents(MIME_CUSTOM2.into(), poll_duration)
                .unwrap();
            assert_eq!(c2.as_slice(), result);

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
        let poll_duration = Duration::from_millis(50);
        let c1 = b"yes files1";
        let c2 = b"yes files2";

        let mut context = ClipboardContext::new().unwrap();
        context.clear().unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context
                .wait_for_target_contents(MIME_CUSTOM1.into(), poll_duration)
                .unwrap();
            assert_eq!(c1.to_vec(), result);
            let result = context
                .wait_for_target_contents(MIME_CUSTOM2.into(), poll_duration)
                .unwrap();
            assert_eq!(c2.to_vec(), result);
            std::thread::sleep(Duration::from_millis(500));
        });

        let mut context = ClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            let mut hash = HashMap::new();
            hash.insert(MIME_CUSTOM1.into(), c1.to_vec());
            context.set_multiple_targets(hash.clone()).unwrap();
            std::thread::sleep(Duration::from_millis(200));
            let mut hash = HashMap::new();
            hash.insert(MIME_CUSTOM2.into(), c2.to_vec());
            context.set_multiple_targets(hash).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_wait_for_non_existing_target() {
        let poll_duration = Duration::from_millis(100);
        let mut context = ClipboardContext::new().unwrap();
        std::thread::spawn(move || {
            context
                .wait_for_target_contents(MIME_CUSTOM1.into(), poll_duration)
                .unwrap();
            panic!("should never happen")
        });
        std::thread::sleep(Duration::from_millis(900));
    }

    #[serial_test::serial]
    #[test]
    fn test_wait_for_non_existing_target_contents_while_changing_selection() {
        let poll_duration = Duration::from_secs(1);
        let c2 = b"yes files2";

        let mut context = ClipboardContext::new().unwrap();
        context.clear().unwrap();

        let _t1 = std::thread::spawn(move || {
            assert!(context
                .wait_for_target_contents(MIME_CUSTOM1.into(), poll_duration)
                .unwrap()
                .is_empty());
            let result = context
                .wait_for_target_contents(MIME_CUSTOM2.into(), poll_duration)
                .unwrap();
            assert_eq!(c2.to_vec(), result);
        });

        let mut context = ClipboardContext::new().unwrap();

        std::thread::sleep(Duration::from_millis(100));

        let t2 = std::thread::spawn(move || {
            let mut hash = Vec::new();
            hash.push((MIME_CUSTOM2.into(), c2.to_vec()));
            context.set_multiple_targets(hash.clone()).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_empty_data_returned_when_targets_change() {
        let poll_duration = Duration::from_secs(1);
        let third_target_data = b"third-target-data";
        let target = MIME_CUSTOM3;

        let mut context = ClipboardContext::new().unwrap();
        context.set_multiple_targets(Vec::new()).unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context
                .get_target_contents(target.into(), poll_duration)
                .unwrap();
            assert!(result.is_empty());

            std::thread::sleep(Duration::from_millis(200));

            let result = context
                .get_target_contents(target.into(), poll_duration)
                .unwrap();
            assert_eq!(result, third_target_data);
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
        let poll_duration = Duration::from_millis(50);
        let third_target_data = b"third-target-data";

        let mut context = ClipboardContext::new().unwrap();
        context.set_multiple_targets(Vec::new()).unwrap();
        context
            .set_target_contents(MIME_CUSTOM1.into(), third_target_data.to_vec())
            .unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context
                .wait_for_target_contents(MIME_CUSTOM2.into(), poll_duration)
                .unwrap();
            assert!(result.is_empty());
        });

        let mut context = ClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            let mut hash = Vec::new();
            hash.push((MIME_CUSTOM3.into(), third_target_data.to_vec()));
            context.set_multiple_targets(hash).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_get_target_contents_return_immediately() {
        let poll_duration = Duration::from_secs(1);
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents(MIME_CUSTOM1.into(), b"initial".to_vec())
            .unwrap();
        assert!(context
            .get_target_contents(MIME_CUSTOM2.into(), poll_duration)
            .unwrap()
            .is_empty());
        assert_eq!(
            context
                .get_target_contents(MIME_CUSTOM1.into(), poll_duration)
                .unwrap(),
            b"initial"
        );
    }

    const BMP_DATA: [u8; 142] = [
        0x42, 0x4d, 0x8e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x8a, 0x00, 0x00, 0x00, 0x7c,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x18, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x23, 0x2e, 0x00, 0x00, 0x23, 0x2e, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0xff,
        0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x42, 0x47, 0x52, 0x73, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0x00,
    ];
}
