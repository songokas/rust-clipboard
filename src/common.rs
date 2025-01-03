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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TargetMimeType {
    Text,
    Bitmap,
    Files,
    // linux: any string
    // windows: number as string:
    // https://docs.rs/clipboard-win/latest/clipboard_win/formats/index.html#constants
    Specific(String),
}

impl From<&str> for TargetMimeType {
    fn from(value: &str) -> Self {
        TargetMimeType::Specific(value.into())
    }
}

pub trait ClipboardProvider: Sized {
    /// create a context with which to access the clipboard
    fn new() -> Result<Self, Box<dyn Error>>;
    /// method to get the clipboard contents as a String
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>>;
    /// method to set the clipboard contents as a String
    fn set_contents(&mut self, contents: String) -> Result<(), Box<dyn Error>>;

    /// get contents by a specific clipboard target
    ///
    /// # Arguments
    ///
    /// * target - clipboard format/target
    /// * poll_duration - how long to wait before returning (x11 clipboard only)
    ///
    /// # Returns
    ///
    /// Result::Ok - contents of the specific target (empty if not found)
    /// Result::Err - any error depending on a clipboard implementation
    fn get_target_contents(
        &mut self,
        target: TargetMimeType,
        poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>>;

    /// wait until clipboard target is available and not empty
    ///
    /// # Arguments
    ///
    /// * target - clipboard format/target
    /// * poll_duration - how long to wait before polling again (no affect on x11)
    ///
    /// # Returns
    ///
    /// when target does not exist returns depending a clipboard implementation
    /// wayland - after 1 second or when the clipboard targets change
    /// x11     - wait indefinitely or until clipboard was updated
    /// windows - after 1 second or when the clipboard targets change
    ///
    /// Result::Ok - contents of the specific target (empty if not found)
    /// Result::Err - any error depending on a clipboard implementation
    fn wait_for_target_contents(
        &mut self,
        target: TargetMimeType,
        poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>>;

    /// set clipboard with a specific target and data
    ///
    /// # Arguments
    ///
    /// * target - clipboard format/target
    /// * data - bytes
    ///
    /// # Returns
    ///
    /// Result::Ok - clipboard successfully updated
    /// Result::Err - any error depending on a clipboard implementation
    fn set_target_contents(
        &mut self,
        target: TargetMimeType,
        data: Vec<u8>,
    ) -> Result<(), Box<dyn Error>>;

    /// set clipboard with multiple specific targets and data
    ///
    /// # Arguments
    ///
    /// * targets - clipboard formats/targets with a specific data
    ///
    /// # Returns
    ///
    /// Result::Ok - clipboard successfully updated
    /// Result::Err - any error depending on a clipboard implementation
    fn set_multiple_targets(
        &mut self,
        targets: impl IntoIterator<Item = (TargetMimeType, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>>;
}
