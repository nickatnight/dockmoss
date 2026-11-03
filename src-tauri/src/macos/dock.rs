//! Every call here is synchronous IPC to the Dock process and can block, so
//! nothing in this module may run on the main thread.
//!
//! The attribute and role names are `CFSTR` macros in the C headers, so
//! `objc2-application-services` has no constants for them. They are built
//! with `CFString::from_static_str` instead.

use std::ptr::{self, NonNull};

use objc2_app_kit::NSRunningApplication;
use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFArray, CFRetained, CFString, CFType, CFURL, CGPoint, CGSize};
use objc2_foundation::{NSBundle, ns_string};

use crate::dock::{DockError, RawDockChild};
use crate::geometry::ScreenRect;

const DOCK_BUNDLE_ID: &str = "com.apple.dock";
const CHILDREN: &str = "AXChildren";
const ROLE: &str = "AXRole";
const SUBROLE: &str = "AXSubrole";
const POSITION: &str = "AXPosition";
const SIZE: &str = "AXSize";
const URL: &str = "AXURL";
const LIST_ROLE: &str = "AXList";

#[derive(Debug, PartialEq, Eq)]
enum Reading {
    Present,
    Missing,
}

pub fn items() -> Result<Vec<RawDockChild>, DockError> {
    let pid = dock_pid().ok_or(DockError::NotRunning)?;
    let dock = application(pid);
    let list = item_list(&dock)?;
    children(&list)?
        .iter()
        .map(|child| raw_child(child))
        .collect()
}

fn dock_pid() -> Option<i32> {
    NSRunningApplication::runningApplicationsWithBundleIdentifier(ns_string!(DOCK_BUNDLE_ID))
        .firstObject()
        .map(|dock| dock.processIdentifier())
}

#[expect(unsafe_code, reason = "AXUIElementCreateApplication is a C function")]
fn application(pid: i32) -> CFRetained<AXUIElement> {
    // SAFETY: AXUIElementCreateApplication accepts any pid and only builds a
    // new element for it (no IPC happens until an attribute is read). The
    // binding wraps the +1 result in `CFRetained`.
    unsafe { AXUIElement::new_application(pid) }
}

fn item_list(dock: &AXUIElement) -> Result<CFRetained<AXUIElement>, DockError> {
    for child in children(dock)? {
        if string_attribute(&child, ROLE)?.as_deref() == Some(LIST_ROLE) {
            return Ok(child);
        }
    }
    Err(DockError::NoItemList)
}

fn raw_child(child: &AXUIElement) -> Result<RawDockChild, DockError> {
    let subrole = string_attribute(child, SUBROLE)?;
    let rect = match (point_attribute(child)?, size_attribute(child)?) {
        (Some(origin), Some(size)) => Some(ScreenRect {
            x: origin.x,
            y: origin.y,
            width: size.width,
            height: size.height,
        }),
        _ => None,
    };
    let url = attribute(child, URL)?.and_then(|value| value.downcast::<CFURL>().ok());
    let app_path = url.as_deref().and_then(CFURL::to_file_path);
    let bundle_id = url.as_deref().and_then(bundle_id);
    Ok(RawDockChild {
        subrole,
        rect,
        app_path,
        bundle_id,
    })
}

fn bundle_id(url: &CFURL) -> Option<String> {
    let bundle = NSBundle::bundleWithURL(url.as_ref())?;
    Some(bundle.bundleIdentifier()?.to_string())
}

#[expect(
    unsafe_code,
    reason = "an AX array holds untyped CF objects that must be viewed as CFType"
)]
fn children(element: &AXUIElement) -> Result<Vec<CFRetained<AXUIElement>>, DockError> {
    let Some(value) = attribute(element, CHILDREN)? else {
        return Ok(Vec::new());
    };
    let Ok(array) = value.downcast::<CFArray>() else {
        return Ok(Vec::new());
    };
    // SAFETY: AXChildren is a CFArray created with the CFType callbacks, so
    // every element is a CF object and viewing it as `CFType` is valid. Each
    // element is then type-checked by `downcast`.
    let array = unsafe { array.cast_unchecked::<CFType>() };
    Ok(array
        .iter()
        .filter_map(|child| child.downcast::<AXUIElement>().ok())
        .collect())
}

fn string_attribute(
    element: &AXUIElement,
    name: &'static str,
) -> Result<Option<String>, DockError> {
    Ok(attribute(element, name)?
        .and_then(|value| value.downcast::<CFString>().ok())
        .map(|string| string.to_string()))
}

#[expect(
    unsafe_code,
    reason = "AXValueGetValue writes the decoded struct through a raw pointer"
)]
fn point_attribute(element: &AXUIElement) -> Result<Option<CGPoint>, DockError> {
    let Some(value) = ax_value(element, POSITION)? else {
        return Ok(None);
    };
    let mut point = CGPoint::ZERO;
    // SAFETY: `point` is a live, writable CGPoint, the struct that
    // AXValueType::CGPoint decodes into. AXValueGetValue checks the stored
    // type and writes nothing when it differs.
    let decoded = unsafe { value.value(AXValueType::CGPoint, NonNull::from(&mut point).cast()) };
    Ok(decoded.then_some(point))
}

#[expect(
    unsafe_code,
    reason = "AXValueGetValue writes the decoded struct through a raw pointer"
)]
fn size_attribute(element: &AXUIElement) -> Result<Option<CGSize>, DockError> {
    let Some(value) = ax_value(element, SIZE)? else {
        return Ok(None);
    };
    let mut size = CGSize::ZERO;
    // SAFETY: `size` is a live, writable CGSize, the struct that
    // AXValueType::CGSize decodes into. AXValueGetValue checks the stored
    // type and writes nothing when it differs.
    let decoded = unsafe { value.value(AXValueType::CGSize, NonNull::from(&mut size).cast()) };
    Ok(decoded.then_some(size))
}

fn ax_value(
    element: &AXUIElement,
    name: &'static str,
) -> Result<Option<CFRetained<AXValue>>, DockError> {
    Ok(attribute(element, name)?.and_then(|value| value.downcast::<AXValue>().ok()))
}

#[expect(
    unsafe_code,
    reason = "AXUIElementCopyAttributeValue returns its value through an out-pointer"
)]
fn attribute(
    element: &AXUIElement,
    name: &'static str,
) -> Result<Option<CFRetained<CFType>>, DockError> {
    let attribute = CFString::from_static_str(name);
    let mut value: *const CFType = ptr::null();
    // SAFETY: `value` is a live, writable out-pointer for the duration of the
    // call, and `element` and `attribute` are valid CF objects.
    let code = unsafe { element.copy_attribute_value(&attribute, NonNull::from(&mut value)) };
    if classify(code, name)? == Reading::Missing {
        return Ok(None);
    }
    let Some(value) = NonNull::new(value.cast_mut()) else {
        return Ok(None);
    };
    // SAFETY: on success AXUIElementCopyAttributeValue stores a CF object
    // that the caller owns (+1, the Copy rule), so `CFRetained` may take
    // over that reference and release it on drop.
    Ok(Some(unsafe { CFRetained::from_raw(value) }))
}

fn classify(code: AXError, attribute: &'static str) -> Result<Reading, DockError> {
    match code {
        AXError::Success => Ok(Reading::Present),
        AXError::NoValue | AXError::AttributeUnsupported => Ok(Reading::Missing),
        AXError::APIDisabled => Err(DockError::ApiDisabled),
        AXError::CannotComplete => Err(DockError::CannotComplete),
        AXError(code) => Err(DockError::Ax { attribute, code }),
    }
}

#[cfg(test)]
mod tests;
