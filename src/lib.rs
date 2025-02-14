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

mod common;
pub use common::{ClipboardProvider, TargetMimeType};

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
))]
pub mod x11_clipboard;

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
))]
pub mod wayland_clipboard;

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
))]
pub mod linux_clipboard;

#[cfg(windows)]
pub mod windows_clipboard;

#[cfg(target_os = "macos")]
pub mod osx_clipboard;

pub mod nop_clipboard;

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
))]
pub type ClipboardContext = linux_clipboard::LinuxClipboardContext;
#[cfg(windows)]
pub type ClipboardContext = windows_clipboard::WindowsClipboardContext;
#[cfg(target_os = "macos")]
pub type ClipboardContext = osx_clipboard::OSXClipboardContext;
#[cfg(target_os = "android")]
pub type ClipboardContext = nop_clipboard::NopClipboardContext; // TODO: implement AndroidClipboardContext (see #52)
#[cfg(not(any(
    unix,
    windows,
    target_os = "macos",
    target_os = "android",
    target_os = "emscripten"
)))]
pub type ClipboardContext = nop_clipboard::NopClipboardContext;

#[test]
fn test_clipboard() {
    let mut ctx = ClipboardContext::new().unwrap();
    ctx.set_contents("some string".to_owned()).unwrap();
    assert!(ctx.get_contents().unwrap() == "some string");
}
