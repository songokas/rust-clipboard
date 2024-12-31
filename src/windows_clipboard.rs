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
                let mut files: Vec<String> =
                    String::from_utf8(data)?.lines().map(|s| s.into()).collect();
                // TODO
                match files.len() {
                    1 => {
                        let files: [String; 1] = files.try_into().unwrap();
                        FileList.write_clipboard(&files)?
                    }
                    2 => {
                        let files: [String; 2] = files.try_into().unwrap();
                        FileList.write_clipboard(&files)?
                    }
                    3 => {
                        let files: [String; 3] = files.try_into().unwrap();
                        FileList.write_clipboard(&files)?
                    }
                    4 => {
                        let files: [String; 4] = files.try_into().unwrap();
                        FileList.write_clipboard(&files)?
                    }
                    5 => {
                        let files: [String; 5] = files.try_into().unwrap();
                        FileList.write_clipboard(&files)?
                    }
                    _ => {
                        files.truncate(6);
                        let files: [String; 6] = files.try_into().unwrap();
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

    /// only 1 target is supported
    fn set_multiple_targets(
        &mut self,
        targets: impl IntoIterator<Item = (TargetMimeType, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>> {
        if let Some((key, value)) = targets.into_iter().next() {
            return self.set_target_contents(key, value);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type ClipboardContext = WindowsClipboardContext;

    #[serial_test::serial]
    #[test]
    fn test_set_target_contents() {
        let pool_duration = Duration::from_secs(1);
        let contents = b"hello test";
        let mut context = ClipboardContext::new().unwrap();
        context.set_target_contents("jumbo", contents).unwrap();
        let result = context.get_target_contents("jumbo", pool_duration).unwrap();
        assert_eq!(contents.to_vec(), result);
    }

    #[serial_test::serial]
    #[test]
    fn test_set_large_target_contents() {
        let pool_duration = Duration::from_secs(1);
        let contents = std::iter::repeat("X").take(100000).collect::<String>();
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("large", contents.as_bytes())
            .unwrap();
        let result = context.get_target_contents("large", pool_duration).unwrap();
        assert_eq!(contents.as_bytes().to_vec(), result);
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
}
