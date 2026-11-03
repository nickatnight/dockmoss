//! Every call here is synchronous IPC to the Dock process and can block, so
//! nothing in this module may run on the main thread.
//!
//! The attribute and role names are `CFSTR` macros in the C headers, so
//! `objc2-application-services` has no constants for them. They are built
//! with `CFString::from_static_str` instead.

use std::collections::HashMap;
use std::path::PathBuf;
use std::ptr::{self, NonNull};

use objc2::rc::autoreleasepool;
use objc2_app_kit::NSRunningApplication;
use objc2_application_services::{
    AXCopyMultipleAttributeOptions, AXError, AXUIElement, AXValue, AXValueType,
};
use objc2_core_foundation::{CFArray, CFRetained, CFString, CFType, CFURL, CGPoint, CGSize};
use objc2_foundation::{NSBundle, ns_string};

use crate::dock::{DockError, RawDockChild};
use crate::geometry::ScreenRect;

const DOCK_BUNDLE_ID: &str = "com.apple.dock";
const DOCK_APP_PATH: &str = "/System/Library/CoreServices/Dock.app";
const CHILDREN: &str = "AXChildren";
const ROLE: &str = "AXRole";
const SUBROLE: &str = "AXSubrole";
const POSITION: &str = "AXPosition";
const SIZE: &str = "AXSize";
const URL: &str = "AXURL";
const CHILD_ATTRIBUTES: &str = "AXSubrole, AXPosition, AXSize and AXURL";
const LIST_ROLE: &str = "AXList";
/// Children do not inherit a timeout set on the application element (checked
/// on macOS 27 with a paused Dock), so it is set on the system-wide element,
/// which sets it for every element this process reads.
const MESSAGING_TIMEOUT_SECONDS: f32 = 0.25;

#[derive(Debug, PartialEq, Eq)]
enum Reading {
    Present,
    Missing,
}

/// Reads the Dock's children, keeping the Dock's application element and
/// each app's bundle ID between reads. Build it on the thread that polls.
pub struct DockReader {
    dock: Option<CFRetained<AXUIElement>>,
    child_attributes: CFRetained<CFArray<CFString>>,
    bundle_ids: HashMap<PathBuf, String>,
}

impl DockReader {
    pub fn new() -> Self {
        set_global_messaging_timeout();
        let names = [SUBROLE, POSITION, SIZE, URL].map(CFString::from_static_str);
        Self {
            dock: None,
            child_attributes: CFArray::from_retained_objects(&names),
            bundle_ids: HashMap::new(),
        }
    }

    /// One full read. Any failure drops the cached Dock element, so the next
    /// read finds the Dock again, as after `killall Dock`.
    pub fn read(&mut self) -> Result<Vec<RawDockChild>, DockError> {
        let read = autoreleasepool(|_| self.read_in_pool());
        if read.is_err() {
            self.dock = None;
        }
        read
    }

    fn read_in_pool(&mut self) -> Result<Vec<RawDockChild>, DockError> {
        if self.dock.is_none() {
            self.dock = Some(find_dock()?);
        }
        let dock = self.dock.as_ref().ok_or(DockError::NotRunning)?;
        let list = item_list(dock)?;
        let mut bundle_ids = HashMap::with_capacity(self.bundle_ids.len());
        let mut kept = Vec::new();
        for child in children(&list)? {
            if let Some(child) = keep_child(self.raw_child(&child, &mut bundle_ids))? {
                kept.push(child);
            }
        }
        self.bundle_ids = bundle_ids;
        Ok(kept)
    }

    /// Reads one child's four attributes in a single AX call. `bundle_ids`
    /// collects this read's IDs, so the cache only holds apps still in the Dock.
    fn raw_child(
        &self,
        child: &AXUIElement,
        bundle_ids: &mut HashMap<PathBuf, String>,
    ) -> Result<RawDockChild, DockError> {
        let [subrole, position, size, url] = child_values(child, &self.child_attributes)?;
        let subrole = subrole
            .and_then(|value| value.downcast::<CFString>().ok())
            .map(|string| string.to_string());
        let rect = match (point(position), size_of(size)) {
            (Some(origin), Some(size)) => Some(ScreenRect {
                x: origin.x,
                y: origin.y,
                width: size.width,
                height: size.height,
            }),
            _ => None,
        };
        let url = url.and_then(|value| value.downcast::<CFURL>().ok());
        let app_path = url.as_deref().and_then(CFURL::to_file_path);
        let bundle_id = match (&app_path, url.as_deref()) {
            (Some(path), Some(url)) => {
                let id = self
                    .bundle_ids
                    .get(path)
                    .cloned()
                    .or_else(|| bundle_id(url));
                if let Some(id) = &id {
                    bundle_ids.insert(path.clone(), id.clone());
                }
                id
            }
            _ => None,
        };
        Ok(RawDockChild {
            subrole,
            rect,
            app_path,
            bundle_id,
        })
    }
}

/// `ApiDisabled` and `CannotComplete` mean the whole Dock is unreadable. Any
/// other error belongs to one child (an app quit between the list read and
/// the child read, say), so only that child is dropped.
fn keep_child(read: Result<RawDockChild, DockError>) -> Result<Option<RawDockChild>, DockError> {
    match read {
        Ok(child) => Ok(Some(child)),
        Err(error @ (DockError::ApiDisabled | DockError::CannotComplete)) => Err(error),
        Err(error) => {
            tracing::debug!(%error, "dock child dropped");
            Ok(None)
        }
    }
}

/// The `com.apple.dock` process whose bundle is the system Dock, so another
/// process that claims the bundle ID is never read.
fn find_dock() -> Result<CFRetained<AXUIElement>, DockError> {
    let pid =
        NSRunningApplication::runningApplicationsWithBundleIdentifier(ns_string!(DOCK_BUNDLE_ID))
            .iter()
            .find(|app| {
                !app.isTerminated()
                    && app
                        .bundleURL()
                        .and_then(|url| url.path())
                        .is_some_and(|path| path.to_string() == DOCK_APP_PATH)
            })
            .map(|dock| dock.processIdentifier())
            .ok_or(DockError::NotRunning)?;
    tracing::debug!(pid, "dock process found");
    Ok(application(pid))
}

#[expect(unsafe_code, reason = "AXUIElementCreateApplication is a C function")]
fn application(pid: i32) -> CFRetained<AXUIElement> {
    // SAFETY: AXUIElementCreateApplication accepts any pid and only builds a
    // new element for it (no IPC happens until an attribute is read). The
    // binding wraps the +1 result in `CFRetained`.
    unsafe { AXUIElement::new_application(pid) }
}

#[expect(
    unsafe_code,
    reason = "the AX system-wide constructor and timeout setter are C functions"
)]
fn set_global_messaging_timeout() {
    // SAFETY: AXUIElementCreateSystemWide takes no arguments and returns a
    // new +1 element, which the binding wraps in `CFRetained`.
    let system_wide = unsafe { AXUIElement::new_system_wide() };
    // SAFETY: `system_wide` is a live AX element and the timeout is positive,
    // as the function requires. Setting it is local to this process (no IPC).
    let code = unsafe { system_wide.set_messaging_timeout(MESSAGING_TIMEOUT_SECONDS) };
    if code == AXError::Success {
        tracing::debug!(
            seconds = MESSAGING_TIMEOUT_SECONDS,
            "ax messaging timeout set"
        );
    } else {
        tracing::warn!(code = code.0, "ax messaging timeout not set");
    }
}

fn item_list(dock: &AXUIElement) -> Result<CFRetained<AXUIElement>, DockError> {
    for child in children(dock)? {
        if string_attribute(&child, ROLE)?.as_deref() == Some(LIST_ROLE) {
            return Ok(child);
        }
    }
    Err(DockError::NoItemList)
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

/// The four child attributes in request order, each `None` when missing.
#[expect(
    unsafe_code,
    reason = "AXUIElementCopyMultipleAttributeValues returns its values through an out-pointer"
)]
fn child_values(
    child: &AXUIElement,
    names: &CFArray<CFString>,
) -> Result<[Option<CFRetained<CFType>>; 4], DockError> {
    let mut values: *const CFArray = ptr::null();
    // SAFETY: `names` is a CFArray of CFStrings, the type the function
    // expects for attribute names, `values` is a live, writable out-pointer
    // for the duration of the call, and `child` is a valid AX element.
    let code = unsafe {
        child.copy_multiple_attribute_values(
            AsRef::<CFArray>::as_ref(names),
            AXCopyMultipleAttributeOptions::empty(),
            NonNull::from(&mut values),
        )
    };
    classify(code, CHILD_ATTRIBUTES)?;
    let Some(values) = NonNull::new(values.cast_mut()) else {
        return Err(DockError::Ax {
            attribute: CHILD_ATTRIBUTES,
            code: AXError::NoValue.0,
        });
    };
    // SAFETY: on success the function stores a CFArray that the caller owns
    // (+1, the Copy rule), so `CFRetained` may take over that reference.
    let values = unsafe { CFRetained::from_raw(values) };
    // SAFETY: the result holds one CF object per requested attribute, in a
    // CFArray created with the CFType callbacks, so viewing each element as
    // `CFType` is valid.
    let values = unsafe { values.cast_unchecked::<CFType>() };
    let mut slots = values.iter();
    Ok([
        slot(slots.next(), SUBROLE)?,
        slot(slots.next(), POSITION)?,
        slot(slots.next(), SIZE)?,
        slot(slots.next(), URL)?,
    ])
}

/// Without `StopOnError`, a missing or failed attribute comes back as an
/// `AXValue` holding its `AXError`, in the attribute's own slot.
fn slot(
    value: Option<CFRetained<CFType>>,
    name: &'static str,
) -> Result<Option<CFRetained<CFType>>, DockError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if let Some(code) = value.downcast_ref::<AXValue>().and_then(ax_error_value)
        && classify(code, name)? == Reading::Missing
    {
        return Ok(None);
    }
    Ok(Some(value))
}

#[expect(
    unsafe_code,
    reason = "AXValueGetValue writes the decoded struct through a raw pointer"
)]
fn ax_error_value(value: &AXValue) -> Option<AXError> {
    let mut code = AXError::Success;
    // SAFETY: `code` is a live, writable AXError (a transparent i32), the
    // type AXValueType::AXError decodes into. AXValueGetValue checks the
    // stored type and writes nothing when it differs.
    let decoded = unsafe { value.value(AXValueType::AXError, NonNull::from(&mut code).cast()) };
    decoded.then_some(code)
}

#[expect(
    unsafe_code,
    reason = "AXValueGetValue writes the decoded struct through a raw pointer"
)]
fn point(value: Option<CFRetained<CFType>>) -> Option<CGPoint> {
    let value = value?.downcast::<AXValue>().ok()?;
    let mut point = CGPoint::ZERO;
    // SAFETY: `point` is a live, writable CGPoint, the struct that
    // AXValueType::CGPoint decodes into. AXValueGetValue checks the stored
    // type and writes nothing when it differs.
    let decoded = unsafe { value.value(AXValueType::CGPoint, NonNull::from(&mut point).cast()) };
    decoded.then_some(point)
}

#[expect(
    unsafe_code,
    reason = "AXValueGetValue writes the decoded struct through a raw pointer"
)]
fn size_of(value: Option<CFRetained<CFType>>) -> Option<CGSize> {
    let value = value?.downcast::<AXValue>().ok()?;
    let mut size = CGSize::ZERO;
    // SAFETY: `size` is a live, writable CGSize, the struct that
    // AXValueType::CGSize decodes into. AXValueGetValue checks the stored
    // type and writes nothing when it differs.
    let decoded = unsafe { value.value(AXValueType::CGSize, NonNull::from(&mut size).cast()) };
    decoded.then_some(size)
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
