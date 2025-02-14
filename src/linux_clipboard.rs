use core::error::Error;
use std::time::Duration;

use crate::common::*;
use crate::wayland_clipboard::WaylandClipboardContext;
use crate::x11_clipboard::{Clipboard, X11ClipboardContext};

enum LinuxContext {
    Wayland(WaylandClipboardContext),
    X11(X11ClipboardContext),
}

pub struct LinuxClipboardContext {
    context: LinuxContext,
}

impl ClipboardProvider for LinuxClipboardContext {
    fn new() -> Result<LinuxClipboardContext, Box<dyn Error>> {
        match WaylandClipboardContext::new() {
            Ok(context) => Ok(LinuxClipboardContext {
                context: LinuxContext::Wayland(context),
            }),
            Err(_) => match X11ClipboardContext::<Clipboard>::new() {
                Ok(context) => Ok(LinuxClipboardContext {
                    context: LinuxContext::X11(context),
                }),
                Err(err) => Err(err),
            },
        }
    }

    fn get_contents(&mut self) -> Result<String, Box<dyn Error>> {
        match &mut self.context {
            LinuxContext::Wayland(context) => context.get_contents(),
            LinuxContext::X11(context) => context.get_contents(),
        }
    }

    fn set_contents(&mut self, content: String) -> Result<(), Box<dyn Error>> {
        match &mut self.context {
            LinuxContext::Wayland(context) => context.set_contents(content),
            LinuxContext::X11(context) => context.set_contents(content),
        }
    }

    fn get_target_contents(
        &mut self,
        target: TargetMimeType,
        poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        match &mut self.context {
            LinuxContext::Wayland(context) => context.get_target_contents(target, poll_duration),
            LinuxContext::X11(context) => context.get_target_contents(target, poll_duration),
        }
    }

    fn wait_for_target_contents(
        &mut self,
        target: TargetMimeType,
        poll_duration: Duration,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        match &mut self.context {
            LinuxContext::Wayland(context) => {
                context.wait_for_target_contents(target, poll_duration)
            }
            LinuxContext::X11(context) => context.wait_for_target_contents(target, poll_duration),
        }
    }

    fn set_target_contents(
        &mut self,
        target: TargetMimeType,
        data: Vec<u8>,
    ) -> Result<(), Box<dyn Error>> {
        match &mut self.context {
            LinuxContext::Wayland(context) => context.set_target_contents(target, data),
            LinuxContext::X11(context) => context.set_target_contents(target, data),
        }
    }

    fn set_multiple_targets(
        &mut self,
        targets: impl IntoIterator<Item = (TargetMimeType, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>> {
        match &mut self.context {
            LinuxContext::Wayland(context) => context.set_multiple_targets(targets),
            LinuxContext::X11(context) => context.set_multiple_targets(targets),
        }
    }

    fn list_targets(&self) -> Result<Vec<TargetMimeType>, Box<dyn Error>> {
        match &self.context {
            LinuxContext::Wayland(context) => context.list_targets(),
            LinuxContext::X11(context) => context.list_targets(),
        }
    }

    fn clear(&mut self) -> Result<(), Box<dyn Error>> {
        match &mut self.context {
            LinuxContext::Wayland(context) => context.clear(),
            LinuxContext::X11(context) => context.clear(),
        }
    }
}
