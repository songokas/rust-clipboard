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

use clipboard_win::formats::Bitmap;
use clipboard_win::formats::FileList;
use clipboard_win::formats::RawData;
use clipboard_win::formats::Unicode;
use clipboard_win::get_clipboard;
use clipboard_win::set_clipboard;
use clipboard_win::Clipboard;
use clipboard_win::Monitor;
use clipboard_win::Setter;
use clipboard_win::{get_clipboard_string, set_clipboard_string};
use std::time::Duration;

use crate::common::TargetMimeType;
use crate::ClipboardProvider;
use std::error::Error;

pub struct WindowsClipboardContext;

impl ClipboardProvider for WindowsClipboardContext {
    fn new() -> Result<Self, Box<dyn Error>> {
        Ok(WindowsClipboardContext)
    }
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        Ok(get_clipboard_string()?)
    }
    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        Ok(set_clipboard_string(&data)?)
    }

    fn get_target_contents(
        &mut self,
        target: TargetMimeType,
        _poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(match target {
            TargetMimeType::Text => get_clipboard(Unicode).map(|s: String| s.into_bytes())?,
            TargetMimeType::Bitmap => get_clipboard(Bitmap)?,
            TargetMimeType::Files => get_clipboard(FileList)
                .map(|list: Vec<String>| list.into_iter().flat_map(|s| s.into_bytes()).collect())?,
            TargetMimeType::Specific(s) => {
                let format_id: u32 = s.parse()?;
                get_clipboard(RawData(format_id))?
            }
        })
    }

    fn wait_for_target_contents(
        &mut self,
        target: TargetMimeType,
        poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut event_received = false;
        loop {
            match self.get_target_contents(target.clone(), poll_duration) {
                Ok(data) if !data.is_empty() => return Ok(data),
                Ok(_) => {
                    if event_received {
                        return Ok(Vec::new());
                    }
                    let _clip = Clipboard::new_attempts(10).expect("Open clipboard");
                    let mut monitor = Monitor::new().expect("create monitor");
                    let Ok(true) = monitor.recv() else {
                        return Ok(Vec::new());
                    };
                    event_received = true;
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
        Ok(match target {
            TargetMimeType::Text => self.set_contents(String::from_utf8(data)?)?,
            TargetMimeType::Bitmap => set_clipboard(Bitmap, data)?,
            TargetMimeType::Files => {
                let content = String::from_utf8(data)?;
                let mut files: Vec<&str> = content.lines().map(|s| s.into()).collect();
                // TODO
                match files.len() {
                    0 => return Ok(()),
                    1 => {
                        let files: [&str; 1] = files.try_into().unwrap();
                        FileList.write_clipboard(&files)?
                    }
                    2 => {
                        let files: [&str; 2] = files.try_into().unwrap();
                        FileList.write_clipboard(&files)?
                    }
                    3 => {
                        let files: [&str; 3] = files.try_into().unwrap();
                        FileList.write_clipboard(&files)?
                    }
                    4 => {
                        let files: [&str; 4] = files.try_into().unwrap();
                        FileList.write_clipboard(&files)?
                    }
                    5 => {
                        let files: [&str; 5] = files.try_into().unwrap();
                        FileList.write_clipboard(&files)?
                    }
                    _ => {
                        files.truncate(6);
                        let files: [&str; 6] = files.try_into().unwrap();
                        FileList.write_clipboard(&files)?
                    }
                }
            }
            TargetMimeType::Specific(s) => {
                let format_id: u32 = s.parse()?;
                set_clipboard(RawData(format_id), data)?
            }
        })
    }

    fn set_multiple_targets(
        &mut self,
        targets: impl IntoIterator<Item = (TargetMimeType, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>> {
        for (key, value) in targets {
            self.set_target_contents(key, value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    type ClipboardContext = WindowsClipboardContext;

    fn get_target() -> String {
        let output = Command::new("powershell")
            .args(&["-command", "Get-Clipboard", "-Raw"])
            .output()
            .expect("failed to execute powershell");
        let contents = String::from_utf8_lossy(&output.stdout);
        contents.to_string()
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
    fn test_set_get_defined_targets() {
        let pool_duration = Duration::from_secs(1);
        let contents = b"hello test";
        let data = [
            (TargetMimeType::Text, ""),
            (TargetMimeType::Files, ""),
            (TargetMimeType::Bitmap, ""),
            (TargetMimeType::Specific("1".to_string()), "x-clipsync"),
        ];
        for (target, expected) in data {
            let mut context = ClipboardContext::new().unwrap();
            context
                .set_target_contents(target.clone(), contents.to_vec())
                .unwrap();
            let result = context.get_target_contents(target, pool_duration).unwrap();
            assert_eq!(contents.as_slice(), result);
            assert_eq!(contents, get_target().as_bytes());
        }
    }

    // #[serial_test::serial]
    // #[test]
    // fn test_set_target_contents() {
    //     let pool_duration = Duration::from_secs(1);
    //     let contents = b"hello test";
    //     let mut context = ClipboardContext::new().unwrap();
    //     context
    //         .set_target_contents("jumbo".into(), contents.to_vec())
    //         .unwrap();
    //     let result = context
    //         .get_target_contents("jumbo".into(), pool_duration)
    //         .unwrap();
    //     assert_eq!(contents.to_vec(), result);
    //     assert_eq!(String::from_utf8_lossy(contents), get_target("jumbo"));
    // }

    // #[serial_test::serial]
    // #[test]
    // fn test_set_large_target_contents() {
    //     let pool_duration = Duration::from_secs(1);
    //     let contents = std::iter::repeat("X").take(100000).collect::<String>();
    //     let mut context = ClipboardContext::new().unwrap();
    //     context
    //         .set_target_contents("large".into(), contents.clone().into_bytes())
    //         .unwrap();
    //     let result = context
    //         .get_target_contents("large".into(), pool_duration)
    //         .unwrap();
    //     assert_eq!(contents.as_bytes().to_vec(), result);
    //     assert_eq!(contents, get_target("large"));
    // }

    // #[serial_test::serial]
    // #[test]
    // fn test_set_multiple_target_contents() {
    //     let pool_duration = Duration::from_secs(1);
    //     let c1 = "yes plain".as_bytes();
    //     let c2 = "yes html".as_bytes();
    //     let c3 = "yes files".as_bytes();
    //     let mut context = ClipboardContext::new().unwrap();
    //     let mut hash = HashMap::new();
    //     hash.insert("jumbo".into(), c1.to_vec());
    //     hash.insert("html".into(), c2.to_vec());
    //     hash.insert("files".into(), c3.to_vec());

    //     context.set_multiple_targets(hash).unwrap();

    //     let result = context
    //         .get_target_contents("jumbo".into(), pool_duration)
    //         .unwrap();
    //     assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
    //     assert_eq!(c1.to_vec(), result);

    //     let result = context
    //         .get_target_contents("html".into(), pool_duration)
    //         .unwrap();
    //     assert_eq!(c2.to_vec(), result);
    //     assert_eq!(String::from_utf8_lossy(c2), get_target("html".into()));

    //     let result = context
    //         .get_target_contents("files".into(), pool_duration)
    //         .unwrap();
    //     assert_eq!(c3.to_vec(), result);
    //     assert_eq!(String::from_utf8_lossy(c3), get_target("files"));
    // }

    // #[serial_test::serial]
    // #[test]
    // fn test_set_multiple_target_contents_with_different_contexts() {
    //     let pool_duration = Duration::from_millis(500);
    //     let c1 = "yes plain".as_bytes();
    //     let c2 = "yes html".as_bytes();
    //     let c3 = "yes files".as_bytes();
    //     let mut context = ClipboardContext::new().unwrap();
    //     let mut hash = HashMap::new();
    //     hash.insert("jumbo".into(), c1.to_vec());
    //     hash.insert("html".into(), c2.to_vec());
    //     hash.insert("files".into(), c3.to_vec());

    //     let t1 = std::thread::spawn(move || {
    //         context.set_multiple_targets(hash).unwrap();
    //         std::thread::sleep(Duration::from_millis(500));
    //     });

    //     let mut context = ClipboardContext::new().unwrap();

    //     let t2 = std::thread::spawn(move || {
    //         let result = context
    //             .get_target_contents("jumbo".into(), pool_duration)
    //             .unwrap();
    //         assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
    //         assert_eq!(c1.to_vec(), result);

    //         let result = context
    //             .get_target_contents("html".into(), pool_duration)
    //             .unwrap();
    //         assert_eq!(c2.to_vec(), result);
    //         assert_eq!(String::from_utf8_lossy(c2), get_target("html"));

    //         let result = context
    //             .get_target_contents("files".into(), pool_duration)
    //             .unwrap();
    //         assert_eq!(c3.to_vec(), result);
    //         assert_eq!(String::from_utf8_lossy(c3), get_target("files"));
    //         std::thread::sleep(Duration::from_millis(500));
    //     });
    //     t1.join().unwrap();
    //     t2.join().unwrap();
    // }

    // #[serial_test::serial]
    // #[test]
    // fn test_wait_for_target_and_obtain_other_targets() {
    //     let pool_duration = Duration::from_secs(1);
    //     let c1 = b"yes plain";
    //     let c2 = b"yes html";
    //     let c3 = b"yes files";
    //     let mut context = ClipboardContext::new().unwrap();
    //     let mut hash = HashMap::new();
    //     hash.insert("jumbo".into(), c1.to_vec());
    //     hash.insert("html".into(), c2.to_vec());
    //     hash.insert("files".into(), c3.to_vec());

    //     let t1 = std::thread::spawn(move || {
    //         let result = context
    //             .wait_for_target_contents("jumbo".into(), pool_duration)
    //             .unwrap();
    //         assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
    //         assert_eq!(c1.to_vec(), result);

    //         let result = context
    //             .get_target_contents("html".into(), pool_duration)
    //             .unwrap();
    //         assert_eq!(c2.to_vec(), result);
    //         assert_eq!(String::from_utf8_lossy(c2), get_target("html"));

    //         let result = context
    //             .get_target_contents("files".into(), pool_duration)
    //             .unwrap();
    //         assert_eq!(c3.to_vec(), result);
    //         assert_eq!(String::from_utf8_lossy(c3), get_target("files"));
    //         std::thread::sleep(Duration::from_millis(500));
    //     });

    //     let mut context = ClipboardContext::new().unwrap();

    //     let t2 = std::thread::spawn(move || {
    //         context.set_multiple_targets(hash).unwrap();
    //         std::thread::sleep(Duration::from_millis(500));
    //     });
    //     t1.join().unwrap();
    //     t2.join().unwrap();
    // }

    // #[serial_test::serial]
    // #[test]
    // fn test_wait_for_target_contents_while_changing_selection() {
    //     let pool_duration = Duration::from_millis(50);
    //     let c1 = b"yes files1";
    //     let c2 = b"yes files2";

    //     let mut context = ClipboardContext::new().unwrap();

    //     let t1 = std::thread::spawn(move || {
    //         let result = context
    //             .wait_for_target_contents("files1".into(), pool_duration)
    //             .unwrap();
    //         assert_eq!(c1.to_vec(), result);
    //         assert_eq!(String::from_utf8_lossy(c1), get_target("files1"));
    //         let result = context
    //             .wait_for_target_contents("files2".into(), pool_duration)
    //             .unwrap();
    //         assert_eq!(c2.to_vec(), result);
    //         assert_eq!(String::from_utf8_lossy(c2), get_target("files2"));
    //         std::thread::sleep(Duration::from_millis(500));
    //     });

    //     let mut context = ClipboardContext::new().unwrap();

    //     let t2 = std::thread::spawn(move || {
    //         let mut hash = HashMap::new();
    //         hash.insert("files1".into(), c1.to_vec());
    //         context.set_multiple_targets(hash.clone()).unwrap();
    //         std::thread::sleep(Duration::from_millis(100));
    //         let mut hash = HashMap::new();
    //         hash.insert("files2".into(), c2.to_vec());
    //         context.set_multiple_targets(hash).unwrap();
    //         std::thread::sleep(Duration::from_millis(500));
    //     });
    //     t1.join().unwrap();
    //     t2.join().unwrap();
    // }

    // #[serial_test::serial]
    // #[test]
    // fn test_wait_for_non_existing_target() {
    //     let pool_duration = Duration::from_millis(100);
    //     let mut context = ClipboardContext::new().unwrap();
    //     std::thread::spawn(move || {
    //         context
    //             .wait_for_target_contents("non-existing-target".into(), pool_duration)
    //             .unwrap();
    //         panic!("should never happen")
    //     });
    //     std::thread::sleep(Duration::from_millis(1000));
    // }

    // #[serial_test::serial]
    // #[test]
    // fn test_wait_for_non_existing_target_contents_while_changing_selection() {
    //     let pool_duration = Duration::from_secs(1);
    //     let c2 = b"yes files2";

    //     let mut context = ClipboardContext::new().unwrap();

    //     let _t1 = std::thread::spawn(move || {
    //         assert!(context
    //             .wait_for_target_contents("files1".into(), pool_duration)
    //             .unwrap()
    //             .is_empty());
    //         let result = context
    //             .wait_for_target_contents("files2".into(), pool_duration)
    //             .unwrap();
    //         assert_eq!(c2.to_vec(), result);
    //         assert_eq!(String::from_utf8_lossy(c2), get_target("files2"));
    //     });

    //     let mut context = ClipboardContext::new().unwrap();

    //     std::thread::sleep(Duration::from_millis(100));

    //     let t2 = std::thread::spawn(move || {
    //         let mut hash = HashMap::new();
    //         hash.insert("files2".into(), c2.to_vec());
    //         context.set_multiple_targets(hash.clone()).unwrap();
    //         std::thread::sleep(Duration::from_millis(500));
    //     });
    //     t2.join().unwrap();
    // }

    // #[serial_test::serial]
    // #[test]
    // fn test_empty_data_returned_when_targets_change() {
    //     let pool_duration = Duration::from_secs(1);
    //     let third_target_data = b"third-target-data";
    //     let target = "third-target";

    //     let mut context = ClipboardContext::new().unwrap();
    //     context
    //         .set_target_contents("initial-target".into(), third_target_data.to_vec())
    //         .unwrap();

    //     let t1 = std::thread::spawn(move || {
    //         let result = context
    //             .get_target_contents(target.into(), pool_duration)
    //             .unwrap();
    //         assert!(result.is_empty());

    //         std::thread::sleep(Duration::from_millis(200));

    //         let result = context
    //             .get_target_contents(target.into(), pool_duration)
    //             .unwrap();
    //         assert_eq!(result, third_target_data);

    //         assert_eq!(
    //             String::from_utf8_lossy(third_target_data),
    //             get_target(target)
    //         );
    //         std::thread::sleep(Duration::from_millis(500));
    //     });
    //     std::thread::sleep(Duration::from_millis(100));
    //     let mut context = ClipboardContext::new().unwrap();

    //     let t2 = std::thread::spawn(move || {
    //         context
    //             .set_target_contents(target.into(), third_target_data.to_vec())
    //             .unwrap();
    //         std::thread::sleep(Duration::from_millis(500));
    //     });
    //     t1.join().unwrap();
    //     t2.join().unwrap();
    // }

    // #[serial_test::serial]
    // #[test]
    // fn test_empty_data_returned_when_multiple_targets_change() {
    //     let pool_duration = Duration::from_millis(50);
    //     let third_target_data = b"third-target-data";

    //     let mut context = ClipboardContext::new().unwrap();
    //     context
    //         .set_target_contents("initial-target".into(), third_target_data.to_vec())
    //         .unwrap();

    //     let t1 = std::thread::spawn(move || {
    //         let result = context
    //             .wait_for_target_contents("second-target".into(), pool_duration)
    //             .unwrap();
    //         assert!(result.is_empty());
    //     });

    //     let mut context = ClipboardContext::new().unwrap();

    //     let t2 = std::thread::spawn(move || {
    //         let mut hash = HashMap::new();
    //         hash.insert("third-target".into(), third_target_data.to_vec());
    //         context.set_multiple_targets(hash).unwrap();
    //         std::thread::sleep(Duration::from_millis(500));
    //     });
    //     t1.join().unwrap();
    //     t2.join().unwrap();
    // }

    // #[serial_test::serial]
    // #[test]
    // fn test_get_target_contents_return_immediately() {
    //     let pool_duration = Duration::from_secs(1);
    //     let mut context = ClipboardContext::new().unwrap();
    //     context
    //         .set_target_contents("initial-target".into(), b"initial".to_vec())
    //         .unwrap();
    //     assert!(context
    //         .get_target_contents("second-target".into(), pool_duration)
    //         .unwrap()
    //         .is_empty());
    //     assert_eq!(
    //         context
    //             .get_target_contents("initial-target".into(), pool_duration)
    //             .unwrap(),
    //         b"initial"
    //     );
    // }
}
