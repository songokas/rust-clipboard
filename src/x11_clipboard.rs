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
use std::collections::HashMap;
use std::error::Error;
use std::marker::PhantomData;
use std::time::Duration;
use x11_clipboard::Atom;
use x11_clipboard::Atoms;
use x11_clipboard::Clipboard as X11Clipboard;

use crate::ClipboardProvider;

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
            Duration::from_secs(3),
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
        target_type: impl ToString,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let target = self.0.getter.get_atom(&target_type.to_string(), true)?;
        if target == 0 {
            return Ok(Vec::new());
        }
        match self.0.load(
            S::atom(&self.0.getter.atoms),
            target,
            self.0.getter.atoms.property,
            Duration::from_secs(1),
        ) {
            Ok(d) => Ok(d),
            Err(x11_clipboard::error::Error::UnexpectedType(_)) => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn wait_for_target_contents(
        &mut self,
        target_type: impl ToString,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let target = loop {
            match self.0.getter.get_atom(&target_type.to_string(), true) {
                Ok(t) if t > 0 => break t,
                _ => {
                    std::thread::park_timeout(Duration::from_millis(50));
                }
            }
        };
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
        target_type: impl ToString,
        data: &[u8],
    ) -> Result<(), Box<dyn Error>> {
        let target = self.0.setter.get_atom(&target_type.to_string(), false)?;
        Ok(self.0.store(S::atom(&self.0.setter.atoms), target, data)?)
    }

    fn set_multiple_targets(
        &mut self,
        targets: HashMap<impl ToString, &[u8]>,
    ) -> Result<(), Box<dyn Error>> {
        let hash: Result<HashMap<_, _>, Box<dyn Error>> = targets
            .into_iter()
            .map(|(key, value)| Ok((self.0.setter.get_atom(&key.to_string(), false)?, value)))
            .collect();
        Ok(self
            .0
            .store_multiple(S::atom(&self.0.setter.atoms), hash?)?)
    }
}

#[cfg(test)]
mod x11clipboard {
    use super::*;
    use std::process::Command;

    type ClipboardContext = X11ClipboardContext;

    // fn list_targets() -> String {
    //     let output = Command::new("xclip")
    //         .args(&["-selection", "clipboard", "-o", "-t", "TARGETS"])
    //         .output()
    //         .expect("failed to execute xclip");
    //     return String::from_utf8_lossy(&output.stdout).to_string();
    // }

    fn get_target(target: &str) -> String {
        let output = Command::new("xclip")
            .args(&["-selection", "clipboard", "-o", "-t", target])
            .output()
            .expect("failed to execute xclip");
        let contents = String::from_utf8_lossy(&output.stdout);
        contents.to_string()
    }

    #[serial_test::serial]
    #[test]
    fn test_set_contents() {
        let contents = "hello test";
        let mut context = ClipboardContext::new().unwrap();
        context.set_contents(contents.to_owned()).unwrap();

        assert_eq!(contents, get_target("UTF8_STRING"));
    }

    #[serial_test::serial]
    #[test]
    fn test_set_target_contents() {
        let contents = b"hello test";
        let mut context = ClipboardContext::new().unwrap();
        context.set_target_contents("jumbo", contents).unwrap();
        let result = context.get_target_contents("jumbo").unwrap();
        assert_eq!(contents.to_vec(), result);
        assert_eq!(
            String::from_utf8_lossy(&contents.to_vec()),
            get_target("jumbo")
        );
    }

    #[serial_test::serial]
    #[test]
    fn test_set_large_target_contents() {
        let contents = std::iter::repeat("X").take(100000).collect::<String>();
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("large", contents.as_bytes())
            .unwrap();
        let result = context.get_target_contents("large").unwrap();
        assert_eq!(contents.as_bytes().to_vec(), result);
        assert_eq!(contents, get_target("large"));
    }

    #[serial_test::serial]
    #[test]
    fn test_set_multiple_target_contents() {
        let c1 = "yes plain".as_bytes();
        let c2 = "yes html".as_bytes();
        let c3 = "yes files".as_bytes();
        let mut context = ClipboardContext::new().unwrap();
        let mut hash = HashMap::new();
        hash.insert("jumbo", c1);
        hash.insert("html", c2);
        hash.insert("files", c3);

        context.set_multiple_targets(hash).unwrap();

        let result = context.get_target_contents("jumbo").unwrap();
        assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
        assert_eq!(c1.to_vec(), result);

        let result = context.get_target_contents("html").unwrap();
        assert_eq!(c2.to_vec(), result);
        assert_eq!(String::from_utf8_lossy(c2), get_target("html"));

        let result = context.get_target_contents("files").unwrap();
        assert_eq!(c3.to_vec(), result);
        assert_eq!(String::from_utf8_lossy(c3), get_target("files"));
    }

    #[serial_test::serial]
    #[test]
    fn test_set_multiple_target_contents_with_different_contexts() {
        let c1 = "yes plain".as_bytes();
        let c2 = "yes html".as_bytes();
        let c3 = "yes files".as_bytes();
        let mut context = ClipboardContext::new().unwrap();
        let mut hash = HashMap::new();
        hash.insert("jumbo", c1);
        hash.insert("html", c2);
        hash.insert("files", c3);

        let t1 = std::thread::spawn(move || {
            context.set_multiple_targets(hash).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });

        let mut context = ClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            let result = context.get_target_contents("jumbo").unwrap();
            assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
            assert_eq!(c1.to_vec(), result);

            let result = context.get_target_contents("html").unwrap();
            assert_eq!(c2.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c2), get_target("html"));

            let result = context.get_target_contents("files").unwrap();
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
        let c1 = "yes plain".as_bytes();
        let c2 = "yes html".as_bytes();
        let c3 = "yes files".as_bytes();
        let mut context = ClipboardContext::new().unwrap();
        let mut hash = HashMap::new();
        hash.insert("jumbo", c1);
        hash.insert("html", c2);
        hash.insert("files", c3);

        let t1 = std::thread::spawn(move || {
            let result = context.wait_for_target_contents("jumbo").unwrap();
            assert_eq!(String::from_utf8_lossy(c1), get_target("jumbo"));
            assert_eq!(c1.to_vec(), result);

            let result = context.get_target_contents("html").unwrap();
            assert_eq!(c2.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c2), get_target("html"));

            let result = context.get_target_contents("files").unwrap();
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
        let c1 = "yes files1".as_bytes();
        let c2 = "yes files2".as_bytes();

        let mut context = ClipboardContext::new().unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context.wait_for_target_contents("files1").unwrap();
            assert_eq!(c1.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c1), get_target("files1"));
            let result = context.wait_for_target_contents("files2").unwrap();
            assert_eq!(c2.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c2), get_target("files2"));
            std::thread::sleep(Duration::from_millis(500));
        });

        let mut context = ClipboardContext::new().unwrap();

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
    fn test_wait_for_non_existing_target() {
        let mut context = ClipboardContext::new().unwrap();
        std::thread::spawn(move || {
            context
                .wait_for_target_contents("non-existing-target")
                .unwrap();
            panic!("should never happen")
        });
        std::thread::sleep(Duration::from_millis(1000));
    }

    #[serial_test::serial]
    #[test]
    fn test_wait_for_non_existing_target_contents_while_changing_selection() {
        let c2 = "yes files2".as_bytes();

        let mut context = ClipboardContext::new().unwrap();

        let _t1 = std::thread::spawn(move || {
            assert!(context
                .wait_for_target_contents("files1")
                .unwrap()
                .is_empty());
            let result = context.wait_for_target_contents("files2").unwrap();
            assert_eq!(c2.to_vec(), result);
            assert_eq!(String::from_utf8_lossy(c2), get_target("files2"));
        });

        let mut context = ClipboardContext::new().unwrap();

        std::thread::sleep(Duration::from_millis(100));

        let t2 = std::thread::spawn(move || {
            let mut hash = HashMap::new();
            hash.insert("files2", c2);
            context.set_multiple_targets(hash.clone()).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_empty_data_returned_when_targets_change() {
        let third_target_data = b"third-target-data";
        let target = "third-target";

        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("initial-target", third_target_data)
            .unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context.get_target_contents(target).unwrap();
            assert!(result.is_empty());

            std::thread::sleep(Duration::from_millis(200));

            let result = context.get_target_contents(target).unwrap();
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
                .set_target_contents(target, third_target_data)
                .unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_empty_data_returned_when_multiple_targets_change() {
        let third_target_data = b"third-target-data";

        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("initial-target", third_target_data)
            .unwrap();

        let t1 = std::thread::spawn(move || {
            let result = context.wait_for_target_contents("second-target").unwrap();
            assert!(result.is_empty());
        });

        let mut context = ClipboardContext::new().unwrap();

        let t2 = std::thread::spawn(move || {
            let mut hash = HashMap::new();
            hash.insert("third-target", third_target_data.as_slice());
            context.set_multiple_targets(hash).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }

    #[serial_test::serial]
    #[test]
    fn test_get_target_contents_return_immediately() {
        let mut context = ClipboardContext::new().unwrap();
        context
            .set_target_contents("initial-target", b"initial")
            .unwrap();
        assert!(context
            .get_target_contents("second-target")
            .unwrap()
            .is_empty());
        assert_eq!(
            context.get_target_contents("initial-target").unwrap(),
            b"initial"
        );
    }
}
