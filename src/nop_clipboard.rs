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

use std::error::Error;

use crate::ClipboardProvider;

pub struct NopClipboardContext;

impl ClipboardProvider for NopClipboardContext {
    fn new() -> Result<NopClipboardContext, Box<dyn Error>> {
        Ok(NopClipboardContext)
    }
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        println!(
            "Attempting to get the contents of the clipboard, which hasn't yet been \
                  implemented on this platform."
        );
        Ok("".to_string())
    }
    fn set_contents(&mut self, _: String) -> Result<(), Box<dyn Error>> {
        println!(
            "Attempting to set the contents of the clipboard, which hasn't yet been \
                  implemented on this platform."
        );
        Ok(())
    }

    fn get_target_contents(
        &mut self,
        _target: crate::common::TargetMimeType,
        _poll_duration: std::time::Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        self.get_contents().map(|s| s.into_bytes())
    }

    fn wait_for_target_contents(
        &mut self,
        target: crate::common::TargetMimeType,
        poll_duration: std::time::Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        self.get_target_contents(target, poll_duration)
    }

    fn set_target_contents(
        &mut self,
        _target: crate::common::TargetMimeType,
        data: Vec<u8>,
    ) -> Result<(), Box<dyn Error>> {
        self.set_contents(String::from_utf8(data)?)
    }

    fn set_multiple_targets(
        &mut self,
        targets: impl IntoIterator<Item = (crate::common::TargetMimeType, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>> {
        if let Some((key, value)) = targets.into_iter().next() {
            return self.set_target_contents(key, value);
        }
        Ok(())
    }
}
