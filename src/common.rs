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
use std::collections::HashMap;

// pub fn err(s: &str) -> Box<dyn Error> {
//     Box::<dyn Error + Send + Sync>::from(s)
// }

/// Trait for clipboard access
pub trait ClipboardProvider: Sized {
    /// Create a context with which to access the clipboard
    // TODO: consider replacing Box<dyn Error> with an associated type?
    fn new() -> Result<Self, Box<dyn Error>>;
    /// Method to get the clipboard contents as a String
    fn get_contents(&mut self) -> Result<String, Box<dyn Error>>;
    /// Method to set the clipboard contents as a String
    fn set_contents(&mut self, String) -> Result<(), Box<dyn Error>>;
    // TODO: come up with some platform-agnostic API for richer types
    // than just strings (c.f. issue #31)

    fn get_target_contents(&mut self, _: impl ToString) -> Result<Vec<u8>, Box<dyn Error>> {
        return self.get_contents().map(|s| s.as_bytes().to_vec())
    }

    fn set_target_contents(&mut self, _: impl ToString, data: &[u8]) -> Result<(), Box<dyn Error>> {
        return self.set_contents(String::from_utf8(data.to_vec())?)
    }

    fn set_multiple_targets(&mut self, targets: HashMap<impl ToString, &[u8]>) -> Result<(), Box<dyn Error>> {
        for (key, value) in targets {
            return self.set_target_contents(key, value);
        }
        return Ok(());
    }
}

