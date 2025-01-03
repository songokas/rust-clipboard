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

use crate::common::*;
use objc::runtime::{Class, Object};
use objc_foundation::{INSArray, INSObject, INSString};
use objc_foundation::{NSArray, NSDictionary, NSObject, NSString};
use objc_id::{Id, Owned};
use std::mem::transmute;

pub struct MacOSClipboardContext {
    pasteboard: Id<Object>,
}

// required to bring NSPasteboard into the path of the class-resolver
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

impl ClipboardProvider for MacOSClipboardContext {
    fn new() -> Result<MacOSClipboardContext, Box<dyn Error>> {
        let cls = Class::get("NSPasteboard").ok_or_else(|| "Class::get(\"NSPasteboard\")")?;
        let pasteboard: *mut Object = unsafe { msg_send![cls, generalPasteboard] };
        if pasteboard.is_null() {
            return Err("NSPasteboard#generalPasteboard returned null".into());
        }
        let pasteboard: Id<Object> = unsafe { Id::from_ptr(pasteboard) };
        Ok(MacOSClipboardContext { pasteboard })
    }

    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        let string_class: Id<NSObject> = {
            let cls: Id<Class> = unsafe { Id::from_ptr(class("NSString")) };
            unsafe { transmute(cls) }
        };
        let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(vec![string_class]);
        let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();
        let string_array: Id<NSArray<NSString>> = unsafe {
            let obj: *mut NSArray<NSString> =
                msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options];
            if obj.is_null() {
                return Err("pasteboard#readObjectsForClasses:options: returned null".into());
            }
            Id::from_ptr(obj)
        };
        if string_array.count() == 0 {
            Err("pasteboard#readObjectsForClasses:options: returned empty".into())
        } else {
            Ok(string_array[0].as_str().to_owned())
        }
    }

    fn set_contents(&mut self, data: String) -> Result<(), Box<dyn Error>> {
        let string_array = NSArray::from_vec(vec![NSString::from_str(&data)]);
        let _: usize = unsafe { msg_send![self.pasteboard, clearContents] };
        let success: bool = unsafe { msg_send![self.pasteboard, writeObjects: string_array] };
        if success {
            Ok(())
        } else {
            Err("NSPasteboard#writeObjects: returned false".into())
        }
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

// this is a convenience function that both cocoa-rs and
//  glutin define, which seems to depend on the fact that
//  Option::None has the same representation as a null pointer
#[inline]
pub fn class(name: &str) -> *mut Class {
    unsafe { transmute(Class::get(name)) }
}
