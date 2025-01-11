use std::error::Error;
use std::thread::sleep;
use std::time::Duration;
use std::time::Instant;

use crate::common::*;
use objc2::rc::Id;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject};
use objc2::{msg_send_id, ClassType};
use objc2_app_kit::{NSImage, NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString, NSURL};

#[allow(dead_code)]
const MIME_TEXT: &str = "public.utf8-plain-text";
#[allow(dead_code)]
const MIME_URI: &str = "public.file-url";
#[allow(dead_code)]
const MIME_BITMAP: &str = "public.tiff";

pub struct OSXClipboardContext {
    pasteboard: Id<NSPasteboard>,
}

impl ClipboardProvider for OSXClipboardContext {
    fn new() -> Result<OSXClipboardContext, Box<dyn Error>> {
        let pasteboard = unsafe { NSPasteboard::generalPasteboard() };
        Ok(OSXClipboardContext { pasteboard })
    }

    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        self.get_target_contents(TargetMimeType::Text, Duration::from_millis(200))
            .and_then(|s| String::from_utf8(s).map_err(Into::into))
    }

    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        self.set_target_contents(TargetMimeType::Text, data.into_bytes())
    }

    fn get_target_contents(
        &mut self,
        target: TargetMimeType,
        _poll_duration: std::time::Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let class_array: Vec<Id<AnyObject>> = match target.clone() {
            TargetMimeType::Text => vec![class_instance(NSString::class())?],
            TargetMimeType::Bitmap => vec![class_instance(NSImage::class())?],
            TargetMimeType::Files => vec![class_instance(NSURL::class())?],
            TargetMimeType::Specific(s) => {
                let uti = NSString::from_str(s.as_str());
                let data = unsafe { self.pasteboard.dataForType(&uti) };
                let Some(data) = data else {
                    return Ok(Vec::new());
                };
                return Ok(data.bytes().to_vec());
            }
        };

        let class_array = NSArray::from_vec(class_array);
        let array = unsafe {
            self.pasteboard
                .readObjectsForClasses_options(&class_array, None)
        };
        let Some(array) = array else {
            return Ok(Vec::new());
        };
        if array.is_empty() {
            return Ok(Vec::new());
        }
        match target {
            TargetMimeType::Text => Ok(array
                .into_iter()
                .filter_map(|o| {
                    // TODO
                    let obj: *const AnyObject = Id::as_ptr(&o);
                    let s = obj as *mut NSString;
                    let sref = unsafe { s.as_ref()? };
                    Some(sref.to_string())
                })
                .collect::<Vec<String>>()
                .join("\n")
                .into_bytes()),
            TargetMimeType::Bitmap => {
                let data: Vec<Vec<u8>> = array
                    .into_iter()
                    .filter_map(|o| {
                        // this must be an NSImage
                        let data: Id<NSData> = unsafe { msg_send_id![&o, TIFFRepresentation] };
                        Some(data.bytes().to_vec())
                    })
                    .collect();
                Ok(data.into_iter().flatten().collect())
            }
            TargetMimeType::Files => {
                let paths: Vec<String> = array
                    .into_iter()
                    .filter_map(|o| {
                        // this must be an NSURL
                        let str: Id<NSString> = unsafe { msg_send_id![&o, absoluteString] };
                        Some(str.to_string())
                    })
                    .collect();
                Ok(paths.join("\n").into_bytes())
            }
            TargetMimeType::Specific(_) => panic!("Specific target is handled above"),
        }
    }

    fn wait_for_target_contents(
        &mut self,
        target: crate::common::TargetMimeType,
        poll_duration: std::time::Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let initial_formats = self.list_targets()?;
        let now = Instant::now();
        loop {
            match self.get_target_contents(target.clone(), poll_duration) {
                Ok(data) if !data.is_empty() => return Ok(data),
                Ok(_) => {
                    if initial_formats != self.list_targets()?
                        || now.elapsed() > Duration::from_millis(999)
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

        let array: Vec<Id<ProtocolObject<dyn NSPasteboardWriting>>> = match target {
            TargetMimeType::Text => {
                vec![ProtocolObject::from_id(create_nsstring(&data)?)]
            }
            TargetMimeType::Bitmap => vec![ProtocolObject::from_id(create_nsimage(data)?)],
            TargetMimeType::Files => create_urls(data)?,
            TargetMimeType::Specific(s) => {
                vec![ProtocolObject::from_id(create_pasteboard_item(s, data)?)]
            }
        };

        let array = NSArray::from_vec(array);
        let success = unsafe { self.pasteboard.writeObjects(&array) };
        if success {
            Ok(())
        } else {
            Err("Unable to set clipboard".into())
        }
    }

    fn set_multiple_targets(
        &mut self,
        targets: impl IntoIterator<Item = (crate::common::TargetMimeType, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>> {
        self.clear()?;
        let array: Result<Vec<Vec<Id<ProtocolObject<dyn NSPasteboardWriting>>>>, Box<dyn Error>> =
            targets
                .into_iter()
                .map(|(target, data)| match target {
                    TargetMimeType::Text => {
                        Ok(vec![ProtocolObject::from_id(create_nsstring(&data)?)])
                    }
                    TargetMimeType::Bitmap => {
                        Ok(vec![ProtocolObject::from_id(create_nsimage(data)?)])
                    }
                    TargetMimeType::Files => Ok(create_urls(data)?),
                    TargetMimeType::Specific(uti) => Ok(vec![ProtocolObject::from_id(
                        create_pasteboard_item(uti, data)?,
                    )]),
                })
                .collect();
        let array: Vec<Id<_>> = array?.into_iter().flatten().collect();
        let array = NSArray::from_vec(array);
        let success = unsafe { self.pasteboard.writeObjects(&array) };
        if success {
            Ok(())
        } else {
            Err("Unable to set clipboard".into())
        }
    }

    fn list_targets(&self) -> Result<Vec<TargetMimeType>, Box<dyn Error>> {
        let array: Option<Id<NSArray<NSString>>> = unsafe { self.pasteboard.types() };
        let Some(array) = array else {
            return Ok(Vec::new());
        };
        Ok(array
            .into_iter()
            .map(|pasteboard_type| TargetMimeType::Specific(pasteboard_type.to_string()))
            .collect())
    }

    fn clear(&mut self) -> Result<(), Box<dyn Error>> {
        let _: isize = unsafe { self.pasteboard.clearContents() };
        Ok(())
    }
}

fn create_urls(
    data: Vec<u8>,
) -> Result<Vec<Id<ProtocolObject<dyn NSPasteboardWriting>>>, Box<dyn Error>> {
    String::from_utf8(data)?
        .lines()
        .map(|l| Ok(ProtocolObject::from_id(create_nsurl(l)?)))
        .collect()
}

fn create_nsstring(data: &[u8]) -> Result<Id<NSString>, Box<dyn Error>> {
    let string = std::str::from_utf8(data)?;
    Ok(NSString::from_str(string))
}

fn create_nsurl(path_str: &str) -> Result<Id<NSURL>, Box<dyn Error>> {
    let path = unsafe { NSURL::fileURLWithPath(&NSString::from_str(path_str)) };
    Ok(path)
}

fn create_nsimage(data: Vec<u8>) -> Result<Id<NSImage>, Box<dyn Error>> {
    let data = NSData::from_vec(data);

    let obj = NSImage::initWithData(NSImage::alloc(), &data);
    let Some(obj) = obj else {
        return Err(format!("Invalid image data provided").into());
    };
    Ok(obj)
}

fn create_pasteboard_item(
    uti: String,
    data: Vec<u8>,
) -> Result<Id<NSPasteboardItem>, Box<dyn Error>> {
    let data = NSData::from_vec(data);
    let for_type = NSString::from_str(uti.as_str());
    let item = unsafe {
        let item = NSPasteboardItem::new();
        if !item.setData_forType(&data, &for_type) {
            return Err(format!("Invalid UTI provided {uti}").into());
        }
        item
    };
    Ok(item)
}

fn class_instance(cls: &AnyClass) -> Result<Id<AnyObject>, Box<dyn Error>> {
    let cls: *const AnyClass = cls;
    let cls = cls as *mut AnyObject;
    Ok(unsafe { Retained::retain(cls).expect("Class must be available") })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, path::absolute, process::Command, time::Duration};

    const MIME_CUSTOM1: &str = "public.text";
    const MIME_CUSTOM2: &str = "public.html";
    const MIME_CUSTOM3: &str = MIME_TEXT;

    type ClipboardContext = OSXClipboardContext;

    fn get_target() -> String {
        let output = Command::new("pbpaste")
            .output()
            .expect("failed to execute xclip");
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
        let poll_duration = Duration::from_secs(1);
        let full_path1 = absolute("osx_clipboard.rs").unwrap();
        let full_path2 = absolute("x11_clipboard.rs").unwrap();
        let image_data: [u8; 43] = [
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xff,
            0xff, 0xff, 0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2c,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00,
            0x3b,
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
                    "file://{}\nfile://{}",
                    full_path1.to_string_lossy(),
                    full_path2.to_string_lossy()
                )
                .into_bytes(),
            ),
            (
                TargetMimeType::Bitmap,
                image_data.to_vec(),
                TIFF_DATA.to_vec(),
            ),
            (
                TargetMimeType::Specific(MIME_TEXT.to_string()),
                b"x-clipsync".to_vec(),
                b"x-clipsync".to_vec(),
            ),
        ];
        for (target, contents, expected) in data {
            let mut context = ClipboardContext::new().unwrap();
            context
                .set_target_contents(target.clone(), contents.clone())
                .unwrap();
            let result = context.get_target_contents(target, poll_duration).unwrap();
            assert_eq!(expected, result);
        }
    }

    #[serial_test::serial]
    #[test]
    fn test_set_unknown_targets() {
        let mut context = ClipboardContext::new().unwrap();
        let contents: Vec<u8> = vec![1, 2, 3, 4];
        assert!(context
            .set_target_contents(
                TargetMimeType::Specific("new-target".to_string()),
                contents.clone(),
            )
            .is_err());
    }

    #[serial_test::serial]
    #[test]
    fn test_set_large_target_contents() {
        let poll_duration = Duration::from_secs(1);
        let contents = std::iter::repeat("X").take(100000).collect::<String>();
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents(MIME_TEXT.into(), contents.clone().into_bytes())
            .unwrap();
        let result = context
            .get_target_contents(MIME_TEXT.into(), poll_duration)
            .unwrap();
        assert_eq!(contents.len(), result.len());
        assert_eq!(contents.as_bytes(), result);
        assert_eq!(contents, get_target());
    }

    #[serial_test::serial]
    #[test]
    fn test_set_multiple_target_contents() {
        let poll_duration = Duration::from_secs(1);
        let c1 = "yes plain";
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
        let mut hash = Vec::new();
        hash.push((MIME_CUSTOM1.into(), c1.to_vec()));
        hash.push((MIME_CUSTOM2.into(), c2.to_vec()));

        let t1 = std::thread::spawn(move || {
            let mut context = ClipboardContext::new().unwrap();
            context.set_multiple_targets(hash).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });

        std::thread::sleep(Duration::from_millis(100));

        let t2 = std::thread::spawn(move || {
            let mut context = ClipboardContext::new().unwrap();

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
            let mut context = ClipboardContext::new().unwrap();
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

        let t2 = std::thread::spawn(move || {
            let mut context = ClipboardContext::new().unwrap();

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

        let t1 = std::thread::spawn(move || {
            let mut context = ClipboardContext::new().unwrap();
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

        let t2 = std::thread::spawn(move || {
            let mut context = ClipboardContext::new().unwrap();
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
        std::thread::spawn(move || {
            let mut context = ClipboardContext::new().unwrap();
            context
                .wait_for_target_contents(MIME_CUSTOM1.into(), poll_duration)
                .unwrap();
            panic!("should never happen")
        });
        std::thread::sleep(Duration::from_millis(1000));
    }

    #[serial_test::serial]
    #[test]
    fn test_wait_for_non_existing_target_contents_while_changing_selection() {
        let poll_duration = Duration::from_secs(1);
        let c2 = b"yes files2";

        let _t1 = std::thread::spawn(move || {
            let mut context = ClipboardContext::new().unwrap();

            assert!(context
                .wait_for_target_contents(MIME_CUSTOM1.into(), poll_duration)
                .unwrap()
                .is_empty());
            let result = context
                .wait_for_target_contents(MIME_CUSTOM2.into(), poll_duration)
                .unwrap();
            assert_eq!(c2.to_vec(), result);
        });

        std::thread::sleep(Duration::from_millis(100));

        let t2 = std::thread::spawn(move || {
            let mut context = ClipboardContext::new().unwrap();

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

        let t1 = std::thread::spawn(move || {
            let mut context = ClipboardContext::new().unwrap();
            context.set_multiple_targets(Vec::new()).unwrap();
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

        let t2 = std::thread::spawn(move || {
            let mut context = ClipboardContext::new().unwrap();

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
            let mut context = ClipboardContext::new().unwrap();
            let result = context
                .wait_for_target_contents(MIME_CUSTOM2.into(), poll_duration)
                .unwrap();
            assert!(result.is_empty());
        });

        let t2 = std::thread::spawn(move || {
            let mut context = ClipboardContext::new().unwrap();
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

    const TIFF_DATA: [u8; 3354] = [
        77, 77, 0, 42, 0, 0, 0, 12, 255, 255, 255, 0, 0, 15, 1, 0, 0, 3, 0, 0, 0, 1, 0, 1, 0, 0, 1,
        1, 0, 3, 0, 0, 0, 1, 0, 1, 0, 0, 1, 2, 0, 3, 0, 0, 0, 3, 0, 0, 0, 198, 1, 3, 0, 3, 0, 0, 0,
        1, 0, 1, 0, 0, 1, 6, 0, 3, 0, 0, 0, 1, 0, 2, 0, 0, 1, 10, 0, 3, 0, 0, 0, 1, 0, 1, 0, 0, 1,
        17, 0, 4, 0, 0, 0, 1, 0, 0, 0, 8, 1, 18, 0, 3, 0, 0, 0, 1, 0, 1, 0, 0, 1, 21, 0, 3, 0, 0,
        0, 1, 0, 3, 0, 0, 1, 22, 0, 3, 0, 0, 0, 1, 0, 1, 0, 0, 1, 23, 0, 4, 0, 0, 0, 1, 0, 0, 0, 3,
        1, 28, 0, 3, 0, 0, 0, 1, 0, 1, 0, 0, 1, 40, 0, 3, 0, 0, 0, 1, 0, 2, 0, 0, 1, 83, 0, 3, 0,
        0, 0, 3, 0, 0, 0, 204, 135, 115, 0, 7, 0, 0, 12, 72, 0, 0, 0, 210, 0, 0, 0, 0, 0, 8, 0, 8,
        0, 8, 0, 1, 0, 1, 0, 1, 0, 0, 12, 72, 76, 105, 110, 111, 2, 16, 0, 0, 109, 110, 116, 114,
        82, 71, 66, 32, 88, 89, 90, 32, 7, 206, 0, 2, 0, 9, 0, 6, 0, 49, 0, 0, 97, 99, 115, 112,
        77, 83, 70, 84, 0, 0, 0, 0, 73, 69, 67, 32, 115, 82, 71, 66, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 246, 214, 0, 1, 0, 0, 0, 0, 211, 45, 72, 80, 32, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 17, 99, 112, 114, 116, 0, 0, 1, 80, 0, 0, 0, 51, 100, 101, 115, 99,
        0, 0, 1, 132, 0, 0, 0, 108, 119, 116, 112, 116, 0, 0, 1, 240, 0, 0, 0, 20, 98, 107, 112,
        116, 0, 0, 2, 4, 0, 0, 0, 20, 114, 88, 89, 90, 0, 0, 2, 24, 0, 0, 0, 20, 103, 88, 89, 90,
        0, 0, 2, 44, 0, 0, 0, 20, 98, 88, 89, 90, 0, 0, 2, 64, 0, 0, 0, 20, 100, 109, 110, 100, 0,
        0, 2, 84, 0, 0, 0, 112, 100, 109, 100, 100, 0, 0, 2, 196, 0, 0, 0, 136, 118, 117, 101, 100,
        0, 0, 3, 76, 0, 0, 0, 134, 118, 105, 101, 119, 0, 0, 3, 212, 0, 0, 0, 36, 108, 117, 109,
        105, 0, 0, 3, 248, 0, 0, 0, 20, 109, 101, 97, 115, 0, 0, 4, 12, 0, 0, 0, 36, 116, 101, 99,
        104, 0, 0, 4, 48, 0, 0, 0, 12, 114, 84, 82, 67, 0, 0, 4, 60, 0, 0, 8, 12, 103, 84, 82, 67,
        0, 0, 4, 60, 0, 0, 8, 12, 98, 84, 82, 67, 0, 0, 4, 60, 0, 0, 8, 12, 116, 101, 120, 116, 0,
        0, 0, 0, 67, 111, 112, 121, 114, 105, 103, 104, 116, 32, 40, 99, 41, 32, 49, 57, 57, 56,
        32, 72, 101, 119, 108, 101, 116, 116, 45, 80, 97, 99, 107, 97, 114, 100, 32, 67, 111, 109,
        112, 97, 110, 121, 0, 0, 100, 101, 115, 99, 0, 0, 0, 0, 0, 0, 0, 18, 115, 82, 71, 66, 32,
        73, 69, 67, 54, 49, 57, 54, 54, 45, 50, 46, 49, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 115,
        82, 71, 66, 32, 73, 69, 67, 54, 49, 57, 54, 54, 45, 50, 46, 49, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 88, 89, 90, 32, 0, 0, 0, 0, 0, 0, 243, 81, 0, 1, 0, 0, 0,
        1, 22, 204, 88, 89, 90, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 88, 89, 90, 32,
        0, 0, 0, 0, 0, 0, 111, 162, 0, 0, 56, 245, 0, 0, 3, 144, 88, 89, 90, 32, 0, 0, 0, 0, 0, 0,
        98, 153, 0, 0, 183, 133, 0, 0, 24, 218, 88, 89, 90, 32, 0, 0, 0, 0, 0, 0, 36, 160, 0, 0,
        15, 132, 0, 0, 182, 207, 100, 101, 115, 99, 0, 0, 0, 0, 0, 0, 0, 22, 73, 69, 67, 32, 104,
        116, 116, 112, 58, 47, 47, 119, 119, 119, 46, 105, 101, 99, 46, 99, 104, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 22, 73, 69, 67, 32, 104, 116, 116, 112, 58, 47, 47, 119, 119, 119, 46, 105,
        101, 99, 46, 99, 104, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 100, 101, 115, 99, 0,
        0, 0, 0, 0, 0, 0, 46, 73, 69, 67, 32, 54, 49, 57, 54, 54, 45, 50, 46, 49, 32, 68, 101, 102,
        97, 117, 108, 116, 32, 82, 71, 66, 32, 99, 111, 108, 111, 117, 114, 32, 115, 112, 97, 99,
        101, 32, 45, 32, 115, 82, 71, 66, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 46, 73, 69, 67, 32, 54,
        49, 57, 54, 54, 45, 50, 46, 49, 32, 68, 101, 102, 97, 117, 108, 116, 32, 82, 71, 66, 32,
        99, 111, 108, 111, 117, 114, 32, 115, 112, 97, 99, 101, 32, 45, 32, 115, 82, 71, 66, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 100, 101, 115, 99, 0, 0, 0, 0,
        0, 0, 0, 44, 82, 101, 102, 101, 114, 101, 110, 99, 101, 32, 86, 105, 101, 119, 105, 110,
        103, 32, 67, 111, 110, 100, 105, 116, 105, 111, 110, 32, 105, 110, 32, 73, 69, 67, 54, 49,
        57, 54, 54, 45, 50, 46, 49, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 44, 82, 101, 102, 101, 114,
        101, 110, 99, 101, 32, 86, 105, 101, 119, 105, 110, 103, 32, 67, 111, 110, 100, 105, 116,
        105, 111, 110, 32, 105, 110, 32, 73, 69, 67, 54, 49, 57, 54, 54, 45, 50, 46, 49, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 118, 105, 101, 119, 0,
        0, 0, 0, 0, 19, 164, 254, 0, 20, 95, 46, 0, 16, 207, 20, 0, 3, 237, 204, 0, 4, 19, 11, 0,
        3, 92, 158, 0, 0, 0, 1, 88, 89, 90, 32, 0, 0, 0, 0, 0, 76, 9, 86, 0, 80, 0, 0, 0, 87, 31,
        231, 109, 101, 97, 115, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 2, 143, 0, 0, 0, 2, 115, 105, 103, 32, 0, 0, 0, 0, 67, 82, 84, 32, 99, 117,
        114, 118, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 5, 0, 10, 0, 15, 0, 20, 0, 25, 0, 30, 0, 35, 0,
        40, 0, 45, 0, 50, 0, 55, 0, 59, 0, 64, 0, 69, 0, 74, 0, 79, 0, 84, 0, 89, 0, 94, 0, 99, 0,
        104, 0, 109, 0, 114, 0, 119, 0, 124, 0, 129, 0, 134, 0, 139, 0, 144, 0, 149, 0, 154, 0,
        159, 0, 164, 0, 169, 0, 174, 0, 178, 0, 183, 0, 188, 0, 193, 0, 198, 0, 203, 0, 208, 0,
        213, 0, 219, 0, 224, 0, 229, 0, 235, 0, 240, 0, 246, 0, 251, 1, 1, 1, 7, 1, 13, 1, 19, 1,
        25, 1, 31, 1, 37, 1, 43, 1, 50, 1, 56, 1, 62, 1, 69, 1, 76, 1, 82, 1, 89, 1, 96, 1, 103, 1,
        110, 1, 117, 1, 124, 1, 131, 1, 139, 1, 146, 1, 154, 1, 161, 1, 169, 1, 177, 1, 185, 1,
        193, 1, 201, 1, 209, 1, 217, 1, 225, 1, 233, 1, 242, 1, 250, 2, 3, 2, 12, 2, 20, 2, 29, 2,
        38, 2, 47, 2, 56, 2, 65, 2, 75, 2, 84, 2, 93, 2, 103, 2, 113, 2, 122, 2, 132, 2, 142, 2,
        152, 2, 162, 2, 172, 2, 182, 2, 193, 2, 203, 2, 213, 2, 224, 2, 235, 2, 245, 3, 0, 3, 11,
        3, 22, 3, 33, 3, 45, 3, 56, 3, 67, 3, 79, 3, 90, 3, 102, 3, 114, 3, 126, 3, 138, 3, 150, 3,
        162, 3, 174, 3, 186, 3, 199, 3, 211, 3, 224, 3, 236, 3, 249, 4, 6, 4, 19, 4, 32, 4, 45, 4,
        59, 4, 72, 4, 85, 4, 99, 4, 113, 4, 126, 4, 140, 4, 154, 4, 168, 4, 182, 4, 196, 4, 211, 4,
        225, 4, 240, 4, 254, 5, 13, 5, 28, 5, 43, 5, 58, 5, 73, 5, 88, 5, 103, 5, 119, 5, 134, 5,
        150, 5, 166, 5, 181, 5, 197, 5, 213, 5, 229, 5, 246, 6, 6, 6, 22, 6, 39, 6, 55, 6, 72, 6,
        89, 6, 106, 6, 123, 6, 140, 6, 157, 6, 175, 6, 192, 6, 209, 6, 227, 6, 245, 7, 7, 7, 25, 7,
        43, 7, 61, 7, 79, 7, 97, 7, 116, 7, 134, 7, 153, 7, 172, 7, 191, 7, 210, 7, 229, 7, 248, 8,
        11, 8, 31, 8, 50, 8, 70, 8, 90, 8, 110, 8, 130, 8, 150, 8, 170, 8, 190, 8, 210, 8, 231, 8,
        251, 9, 16, 9, 37, 9, 58, 9, 79, 9, 100, 9, 121, 9, 143, 9, 164, 9, 186, 9, 207, 9, 229, 9,
        251, 10, 17, 10, 39, 10, 61, 10, 84, 10, 106, 10, 129, 10, 152, 10, 174, 10, 197, 10, 220,
        10, 243, 11, 11, 11, 34, 11, 57, 11, 81, 11, 105, 11, 128, 11, 152, 11, 176, 11, 200, 11,
        225, 11, 249, 12, 18, 12, 42, 12, 67, 12, 92, 12, 117, 12, 142, 12, 167, 12, 192, 12, 217,
        12, 243, 13, 13, 13, 38, 13, 64, 13, 90, 13, 116, 13, 142, 13, 169, 13, 195, 13, 222, 13,
        248, 14, 19, 14, 46, 14, 73, 14, 100, 14, 127, 14, 155, 14, 182, 14, 210, 14, 238, 15, 9,
        15, 37, 15, 65, 15, 94, 15, 122, 15, 150, 15, 179, 15, 207, 15, 236, 16, 9, 16, 38, 16, 67,
        16, 97, 16, 126, 16, 155, 16, 185, 16, 215, 16, 245, 17, 19, 17, 49, 17, 79, 17, 109, 17,
        140, 17, 170, 17, 201, 17, 232, 18, 7, 18, 38, 18, 69, 18, 100, 18, 132, 18, 163, 18, 195,
        18, 227, 19, 3, 19, 35, 19, 67, 19, 99, 19, 131, 19, 164, 19, 197, 19, 229, 20, 6, 20, 39,
        20, 73, 20, 106, 20, 139, 20, 173, 20, 206, 20, 240, 21, 18, 21, 52, 21, 86, 21, 120, 21,
        155, 21, 189, 21, 224, 22, 3, 22, 38, 22, 73, 22, 108, 22, 143, 22, 178, 22, 214, 22, 250,
        23, 29, 23, 65, 23, 101, 23, 137, 23, 174, 23, 210, 23, 247, 24, 27, 24, 64, 24, 101, 24,
        138, 24, 175, 24, 213, 24, 250, 25, 32, 25, 69, 25, 107, 25, 145, 25, 183, 25, 221, 26, 4,
        26, 42, 26, 81, 26, 119, 26, 158, 26, 197, 26, 236, 27, 20, 27, 59, 27, 99, 27, 138, 27,
        178, 27, 218, 28, 2, 28, 42, 28, 82, 28, 123, 28, 163, 28, 204, 28, 245, 29, 30, 29, 71,
        29, 112, 29, 153, 29, 195, 29, 236, 30, 22, 30, 64, 30, 106, 30, 148, 30, 190, 30, 233, 31,
        19, 31, 62, 31, 105, 31, 148, 31, 191, 31, 234, 32, 21, 32, 65, 32, 108, 32, 152, 32, 196,
        32, 240, 33, 28, 33, 72, 33, 117, 33, 161, 33, 206, 33, 251, 34, 39, 34, 85, 34, 130, 34,
        175, 34, 221, 35, 10, 35, 56, 35, 102, 35, 148, 35, 194, 35, 240, 36, 31, 36, 77, 36, 124,
        36, 171, 36, 218, 37, 9, 37, 56, 37, 104, 37, 151, 37, 199, 37, 247, 38, 39, 38, 87, 38,
        135, 38, 183, 38, 232, 39, 24, 39, 73, 39, 122, 39, 171, 39, 220, 40, 13, 40, 63, 40, 113,
        40, 162, 40, 212, 41, 6, 41, 56, 41, 107, 41, 157, 41, 208, 42, 2, 42, 53, 42, 104, 42,
        155, 42, 207, 43, 2, 43, 54, 43, 105, 43, 157, 43, 209, 44, 5, 44, 57, 44, 110, 44, 162,
        44, 215, 45, 12, 45, 65, 45, 118, 45, 171, 45, 225, 46, 22, 46, 76, 46, 130, 46, 183, 46,
        238, 47, 36, 47, 90, 47, 145, 47, 199, 47, 254, 48, 53, 48, 108, 48, 164, 48, 219, 49, 18,
        49, 74, 49, 130, 49, 186, 49, 242, 50, 42, 50, 99, 50, 155, 50, 212, 51, 13, 51, 70, 51,
        127, 51, 184, 51, 241, 52, 43, 52, 101, 52, 158, 52, 216, 53, 19, 53, 77, 53, 135, 53, 194,
        53, 253, 54, 55, 54, 114, 54, 174, 54, 233, 55, 36, 55, 96, 55, 156, 55, 215, 56, 20, 56,
        80, 56, 140, 56, 200, 57, 5, 57, 66, 57, 127, 57, 188, 57, 249, 58, 54, 58, 116, 58, 178,
        58, 239, 59, 45, 59, 107, 59, 170, 59, 232, 60, 39, 60, 101, 60, 164, 60, 227, 61, 34, 61,
        97, 61, 161, 61, 224, 62, 32, 62, 96, 62, 160, 62, 224, 63, 33, 63, 97, 63, 162, 63, 226,
        64, 35, 64, 100, 64, 166, 64, 231, 65, 41, 65, 106, 65, 172, 65, 238, 66, 48, 66, 114, 66,
        181, 66, 247, 67, 58, 67, 125, 67, 192, 68, 3, 68, 71, 68, 138, 68, 206, 69, 18, 69, 85,
        69, 154, 69, 222, 70, 34, 70, 103, 70, 171, 70, 240, 71, 53, 71, 123, 71, 192, 72, 5, 72,
        75, 72, 145, 72, 215, 73, 29, 73, 99, 73, 169, 73, 240, 74, 55, 74, 125, 74, 196, 75, 12,
        75, 83, 75, 154, 75, 226, 76, 42, 76, 114, 76, 186, 77, 2, 77, 74, 77, 147, 77, 220, 78,
        37, 78, 110, 78, 183, 79, 0, 79, 73, 79, 147, 79, 221, 80, 39, 80, 113, 80, 187, 81, 6, 81,
        80, 81, 155, 81, 230, 82, 49, 82, 124, 82, 199, 83, 19, 83, 95, 83, 170, 83, 246, 84, 66,
        84, 143, 84, 219, 85, 40, 85, 117, 85, 194, 86, 15, 86, 92, 86, 169, 86, 247, 87, 68, 87,
        146, 87, 224, 88, 47, 88, 125, 88, 203, 89, 26, 89, 105, 89, 184, 90, 7, 90, 86, 90, 166,
        90, 245, 91, 69, 91, 149, 91, 229, 92, 53, 92, 134, 92, 214, 93, 39, 93, 120, 93, 201, 94,
        26, 94, 108, 94, 189, 95, 15, 95, 97, 95, 179, 96, 5, 96, 87, 96, 170, 96, 252, 97, 79, 97,
        162, 97, 245, 98, 73, 98, 156, 98, 240, 99, 67, 99, 151, 99, 235, 100, 64, 100, 148, 100,
        233, 101, 61, 101, 146, 101, 231, 102, 61, 102, 146, 102, 232, 103, 61, 103, 147, 103, 233,
        104, 63, 104, 150, 104, 236, 105, 67, 105, 154, 105, 241, 106, 72, 106, 159, 106, 247, 107,
        79, 107, 167, 107, 255, 108, 87, 108, 175, 109, 8, 109, 96, 109, 185, 110, 18, 110, 107,
        110, 196, 111, 30, 111, 120, 111, 209, 112, 43, 112, 134, 112, 224, 113, 58, 113, 149, 113,
        240, 114, 75, 114, 166, 115, 1, 115, 93, 115, 184, 116, 20, 116, 112, 116, 204, 117, 40,
        117, 133, 117, 225, 118, 62, 118, 155, 118, 248, 119, 86, 119, 179, 120, 17, 120, 110, 120,
        204, 121, 42, 121, 137, 121, 231, 122, 70, 122, 165, 123, 4, 123, 99, 123, 194, 124, 33,
        124, 129, 124, 225, 125, 65, 125, 161, 126, 1, 126, 98, 126, 194, 127, 35, 127, 132, 127,
        229, 128, 71, 128, 168, 129, 10, 129, 107, 129, 205, 130, 48, 130, 146, 130, 244, 131, 87,
        131, 186, 132, 29, 132, 128, 132, 227, 133, 71, 133, 171, 134, 14, 134, 114, 134, 215, 135,
        59, 135, 159, 136, 4, 136, 105, 136, 206, 137, 51, 137, 153, 137, 254, 138, 100, 138, 202,
        139, 48, 139, 150, 139, 252, 140, 99, 140, 202, 141, 49, 141, 152, 141, 255, 142, 102, 142,
        206, 143, 54, 143, 158, 144, 6, 144, 110, 144, 214, 145, 63, 145, 168, 146, 17, 146, 122,
        146, 227, 147, 77, 147, 182, 148, 32, 148, 138, 148, 244, 149, 95, 149, 201, 150, 52, 150,
        159, 151, 10, 151, 117, 151, 224, 152, 76, 152, 184, 153, 36, 153, 144, 153, 252, 154, 104,
        154, 213, 155, 66, 155, 175, 156, 28, 156, 137, 156, 247, 157, 100, 157, 210, 158, 64, 158,
        174, 159, 29, 159, 139, 159, 250, 160, 105, 160, 216, 161, 71, 161, 182, 162, 38, 162, 150,
        163, 6, 163, 118, 163, 230, 164, 86, 164, 199, 165, 56, 165, 169, 166, 26, 166, 139, 166,
        253, 167, 110, 167, 224, 168, 82, 168, 196, 169, 55, 169, 169, 170, 28, 170, 143, 171, 2,
        171, 117, 171, 233, 172, 92, 172, 208, 173, 68, 173, 184, 174, 45, 174, 161, 175, 22, 175,
        139, 176, 0, 176, 117, 176, 234, 177, 96, 177, 214, 178, 75, 178, 194, 179, 56, 179, 174,
        180, 37, 180, 156, 181, 19, 181, 138, 182, 1, 182, 121, 182, 240, 183, 104, 183, 224, 184,
        89, 184, 209, 185, 74, 185, 194, 186, 59, 186, 181, 187, 46, 187, 167, 188, 33, 188, 155,
        189, 21, 189, 143, 190, 10, 190, 132, 190, 255, 191, 122, 191, 245, 192, 112, 192, 236,
        193, 103, 193, 227, 194, 95, 194, 219, 195, 88, 195, 212, 196, 81, 196, 206, 197, 75, 197,
        200, 198, 70, 198, 195, 199, 65, 199, 191, 200, 61, 200, 188, 201, 58, 201, 185, 202, 56,
        202, 183, 203, 54, 203, 182, 204, 53, 204, 181, 205, 53, 205, 181, 206, 54, 206, 182, 207,
        55, 207, 184, 208, 57, 208, 186, 209, 60, 209, 190, 210, 63, 210, 193, 211, 68, 211, 198,
        212, 73, 212, 203, 213, 78, 213, 209, 214, 85, 214, 216, 215, 92, 215, 224, 216, 100, 216,
        232, 217, 108, 217, 241, 218, 118, 218, 251, 219, 128, 220, 5, 220, 138, 221, 16, 221, 150,
        222, 28, 222, 162, 223, 41, 223, 175, 224, 54, 224, 189, 225, 68, 225, 204, 226, 83, 226,
        219, 227, 99, 227, 235, 228, 115, 228, 252, 229, 132, 230, 13, 230, 150, 231, 31, 231, 169,
        232, 50, 232, 188, 233, 70, 233, 208, 234, 91, 234, 229, 235, 112, 235, 251, 236, 134, 237,
        17, 237, 156, 238, 40, 238, 180, 239, 64, 239, 204, 240, 88, 240, 229, 241, 114, 241, 255,
        242, 140, 243, 25, 243, 167, 244, 52, 244, 194, 245, 80, 245, 222, 246, 109, 246, 251, 247,
        138, 248, 25, 248, 168, 249, 56, 249, 199, 250, 87, 250, 231, 251, 119, 252, 7, 252, 152,
        253, 41, 253, 186, 254, 75, 254, 220, 255, 109, 255, 255,
    ];
}
