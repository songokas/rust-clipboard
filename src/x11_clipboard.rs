/*
Copyright 2017 Avraham Weinstock

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

// use common::*;
use std::error::Error;
use std::marker::PhantomData;
use std::time::Duration;
use x11_clipboard::Atom;
use x11_clipboard::Atoms;
use x11_clipboard::Clipboard as X11Clipboard;

use crate::common::TargetMimeType;
use crate::ClipboardProvider;

#[allow(dead_code)]
const MIME_TEXT: &str = "UTF8_STRING";
const MIME_URI: &str = "text/uri-list";
const MIME_BITMAP: &str = "image/png";

pub trait Selection {
    fn atom(atoms: &Atoms) -> Atom;
}

pub struct Primary;

impl Selection for Primary {
    fn atom(atoms: &Atoms) -> Atom {
        atoms.primary
    }
}

pub struct Clipboard;

impl Selection for Clipboard {
    fn atom(atoms: &Atoms) -> Atom {
        atoms.clipboard
    }
}

pub struct X11ClipboardContext<S = Clipboard>(X11Clipboard, PhantomData<S>)
where
    S: Selection;

impl<S> X11ClipboardContext<S>
where
    S: Selection,
{
    fn get_target(&self, target: TargetMimeType) -> Result<Atom, x11_clipboard::error::Error> {
        match target {
            TargetMimeType::Text => Ok(self.0.getter.atoms.utf8_string),
            TargetMimeType::Bitmap => self.0.getter.get_atom(MIME_BITMAP, false),
            TargetMimeType::Files => self.0.getter.get_atom(MIME_URI, false),
            TargetMimeType::Specific(s) => self.0.getter.get_atom(&s, false),
        }
    }
}

impl<S> ClipboardProvider for X11ClipboardContext<S>
where
    S: Selection,
{
    fn new() -> Result<X11ClipboardContext<S>, Box<dyn Error>> {
        Ok(X11ClipboardContext(X11Clipboard::new()?, PhantomData))
    }

    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        Ok(String::from_utf8(self.0.load(
            S::atom(&self.0.getter.atoms),
            self.0.getter.atoms.utf8_string,
            self.0.getter.atoms.property,
            Duration::from_millis(1000),
        )?)?)
    }

    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        Ok(self.0.store(
            S::atom(&self.0.setter.atoms),
            self.0.setter.atoms.utf8_string,
            data,
        )?)
    }

    fn get_target_contents(
        &mut self,
        target: TargetMimeType,
        poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let target = match target {
            TargetMimeType::Text => self.0.getter.atoms.utf8_string,
            TargetMimeType::Bitmap => self.0.getter.get_atom(MIME_BITMAP, true)?,
            TargetMimeType::Files => self.0.getter.get_atom(MIME_URI, true)?,
            TargetMimeType::Specific(s) => self.0.getter.get_atom(&s, true)?,
        };

        if target == 0 {
            return Ok(Vec::new());
        }
        match self.0.load(
            S::atom(&self.0.getter.atoms),
            target,
            self.0.getter.atoms.property,
            poll_duration,
        ) {
            Ok(d) => Ok(d),
            Err(x11_clipboard::error::Error::UnexpectedType(_)) => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn wait_for_target_contents(
        &mut self,
        target: TargetMimeType,
        _poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        // rely on load wait to return once clipboard is modified
        let target = self.get_target(target)?;
        match self.0.load_wait(
            S::atom(&self.0.getter.atoms),
            target,
            self.0.getter.atoms.property,
        ) {
            Ok(d) => Ok(d),
            Err(x11_clipboard::error::Error::UnexpectedType(_)) => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn set_target_contents(
        &mut self,
        target: TargetMimeType,
        data: Vec<u8>,
    ) -> Result<(), Box<dyn Error>> {
        let target = self.get_target(target)?;
        Ok(self.0.store(S::atom(&self.0.setter.atoms), target, data)?)
    }

    fn set_multiple_targets(
        &mut self,
        targets: impl IntoIterator<Item = (TargetMimeType, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>> {
        let hash: Result<Vec<_>, Box<dyn Error>> = targets
            .into_iter()
            .map(|(target, value)| Ok((self.get_target(target)?, value)))
            .collect();
        Ok(self
            .0
            .store_multiple(S::atom(&self.0.setter.atoms), hash?)?)
    }

    fn list_targets(&self) -> Result<Vec<TargetMimeType>, Box<dyn Error>> {
        let content = self.0.list_target_names(
            S::atom(&self.0.setter.atoms),
            Duration::from_millis(100).into(),
        )?;
        content
            .into_iter()
            .map(|s| Ok(TargetMimeType::Specific(String::from_utf8(s)?)))
            .collect()
    }

    fn clear(&mut self) -> Result<(), Box<dyn Error>> {
        self.0
            .clear(S::atom(&self.0.setter.atoms))
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, process::Command};

    type ClipboardContext = X11ClipboardContext;

    fn list_targets() -> String {
        let output = Command::new("xclip")
            .args(["-selection", "clipboard", "-o", "-t", "TARGETS"])
            .output()
            .expect("failed to execute xclip");
        return String::from_utf8_lossy(&output.stdout).to_string();
    }

    fn get_target(target: &str) -> String {
        let output = Command::new("xclip")
            .args(["-selection", "clipboard", "-o", "-t", target])
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
        assert_eq!(contents, get_target("UTF8_STRING"));
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
        assert_eq!(targets, list_targets().trim_end());
    }

    #[serial_test::serial]
    #[test]
    fn test_set_get_defined_targets() {
        let poll_duration = Duration::from_secs(1);
        let contents = b"hello test";
        let data = [
            (TargetMimeType::Text, MIME_TEXT),
            (TargetMimeType::Files, MIME_URI),
            (TargetMimeType::Bitmap, MIME_BITMAP),
            (
                TargetMimeType::Specific("x-clipsync".to_string()),
                "x-clipsync",
            ),
        ];
        let mut context = ClipboardContext::new().unwrap();
        for (target, expected) in data {
            context
                .set_target_contents(target.clone(), contents.to_vec())
                .unwrap();
            let result = context.get_target_contents(target, poll_duration).unwrap();
            assert_eq!(contents.as_slice(), result);
            assert_eq!(contents, get_target(expected).as_bytes());
        }
    }

    #[serial_test::serial]
    #[test]
    fn test_set_target_contents() {
        let poll_duration = Duration::from_secs(1);
        let contents = b"hello test";
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("jumbo".into(), contents.to_vec())
            .unwrap();
        let result = context
            .get_target_contents("jumbo".into(), poll_duration)
            .unwrap();
        assert_eq!(contents.to_vec(), result);
        assert_eq!(String::from_utf8_lossy(contents), get_target("jumbo"));
    }

    #[serial_test::serial]
    #[test]
    fn test_set_large_target_contents() {
        let poll_duration = Duration::from_secs(1);
        let contents = "X".repeat(100000);
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("large".into(), contents.clone().into_bytes())
            .unwrap();
        let result = context
            .get_target_contents("large".into(), poll_duration)
            .unwrap();
        assert_eq!(contents.as_bytes().to_vec(), result);
        assert_eq!(contents, get_target("large"));
    }

    #[serial_test::serial]
    #[test]
    fn test_set_multiple_target_contents() {
        let poll_duration = Duration::from_secs(1);
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
            .get_target_contents("jumbo".into(), poll_duration)
            .unwrap();
        assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
        assert_eq!(c1.to_vec(), result);

        let result = context
            .get_target_contents("html".into(), poll_duration)
            .unwrap();
        assert_eq!(c2.to_vec(), result);
        assert_eq!(String::from_utf8_lossy(c2), get_target("html"));

        let result = context
            .get_target_contents("files".into(), poll_duration)
            .unwrap();
        assert_eq!(c3.to_vec(), result);
        assert_eq!(String::from_utf8_lossy(c3), get_target("files"));
    }

    #[serial_test::serial]
    #[test]
    fn test_set_multiple_target_contents_with_different_contexts() {
        let poll_duration = Duration::from_millis(500);
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

        std::thread::sleep(Duration::from_millis(100));
        let mut context = ClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            let result = context
                .get_target_contents("jumbo".into(), poll_duration)
                .unwrap();
            assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
            assert_eq!(c1.to_vec(), result);

            let result = context
                .get_target_contents("html".into(), poll_duration)
                .unwrap();
            assert_eq!(c2.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c2), get_target("html"));

            let result = context
                .get_target_contents("files".into(), poll_duration)
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
        let poll_duration = Duration::from_secs(1);
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
                .wait_for_target_contents("jumbo".into(), poll_duration)
                .unwrap();
            assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
            assert_eq!(c1.to_vec(), result);

            let result = context
                .get_target_contents("html".into(), poll_duration)
                .unwrap();
            assert_eq!(c2.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c2), get_target("html"));

            let result = context
                .get_target_contents("files".into(), poll_duration)
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
        let poll_duration = Duration::from_millis(50);
        let c1 = b"yes files1";
        let c2 = b"yes files2";

        let mut context = ClipboardContext::new().unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context
                .wait_for_target_contents("files1".into(), poll_duration)
                .unwrap();
            assert_eq!(c1.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c1), get_target("files1"));
            let result = context
                .wait_for_target_contents("files2".into(), poll_duration)
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
        let poll_duration = Duration::from_millis(100);
        let mut context = ClipboardContext::new().unwrap();
        std::thread::spawn(move || {
            context
                .wait_for_target_contents("non-existing-target".into(), poll_duration)
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

        let mut context = ClipboardContext::new().unwrap();

        let _t1 = std::thread::spawn(move || {
            assert!(context
                .wait_for_target_contents("files1".into(), poll_duration)
                .unwrap()
                .is_empty());
            let result = context
                .wait_for_target_contents("files2".into(), poll_duration)
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
        let poll_duration = Duration::from_secs(1);
        let third_target_data = b"third-target-data";
        let target = "third-target";

        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("initial-target".into(), third_target_data.to_vec())
            .unwrap();

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
        let poll_duration = Duration::from_millis(50);
        let third_target_data = b"third-target-data";

        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("initial-target".into(), third_target_data.to_vec())
            .unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context
                .wait_for_target_contents("second-target".into(), poll_duration)
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
        let poll_duration = Duration::from_secs(1);
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("initial-target".into(), b"initial".to_vec())
            .unwrap();
        assert!(context
            .get_target_contents("second-target".into(), poll_duration)
            .unwrap()
            .is_empty());
        assert_eq!(
            context
                .get_target_contents("initial-target".into(), poll_duration)
                .unwrap(),
            b"initial"
        );
    }
}
