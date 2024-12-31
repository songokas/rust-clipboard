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

use core::time::Duration;
use std::error::Error;

#[derive(Debug, Clone)]
pub enum TargetMimeType {
    Text,
    Bitmap,
    Files,
    Specific(String),
}

/// Trait for clipboard access
pub trait ClipboardProvider: Sized {
    /// Create a context with which to access the clipboard
    fn new() -> Result<Self, Box<dyn Error>>;
    /// Method to get the clipboard contents as a String
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>>;
    /// Method to set the clipboard contents as a String
    fn set_contents(&mut self, contents: String) -> Result<(), Box<dyn Error>>;

    /// returns target contents
    fn get_target_contents(
        &mut self,
        target: TargetMimeType,
        poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>>;
    //     return self.get_contents().map(|s| s.as_bytes().to_vec());
    // }

    /// wait until target is available and not empty
    /// returns if clipboard was updated even if the target requested is not available
    fn wait_for_target_contents(
        &mut self,
        target: TargetMimeType,
        poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>>;

    fn set_target_contents(
        &mut self,
        target: TargetMimeType,
        data: Vec<u8>,
    ) -> Result<(), Box<dyn Error>>;

    fn set_multiple_targets(
        &mut self,
        targets: impl IntoIterator<Item = (TargetMimeType, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>>;
}
