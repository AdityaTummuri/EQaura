//! Thin, checked wrappers around Core Audio.

use crate::Device;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_audio::*;
use objc2_core_foundation::{CFDictionary, CFRetained, CFString};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

pub type Id = AudioObjectID;
const SYSTEM: Id = kAudioObjectSystemObject as Id;

fn address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// Turns 'nope' style codes into readable text.
pub fn check(what: &str, status: i32) -> Result<(), String> {
    if status == 0 {
        return Ok(());
    }
    let b = status.to_be_bytes();
    let code = if b.iter().all(|c| c.is_ascii_graphic() || *c == b' ') {
        String::from_utf8_lossy(&b).into_owned()
    } else {
        status.to_string()
    };
    Err(format!("{what} failed ({code})"))
}

/// Reads a plain-data property. `T` must be valid for any bit pattern.
fn get<T: Copy + Default>(id: Id, selector: u32, scope: u32) -> Option<T> {
    let mut addr = address(selector, scope);
    let mut size = size_of::<T>() as u32;
    let mut value = T::default();
    // SAFETY: size and pointer describe `value`.
    let status = unsafe {
        AudioObjectGetPropertyData(
            id,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    (status == 0 && size as usize == size_of::<T>()).then_some(value)
}

fn get_list(id: Id, selector: u32, scope: u32) -> Vec<Id> {
    let mut addr = address(selector, scope);
    let mut size = 0u32;
    // SAFETY: only the size is written.
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            id,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
        )
    };
    if status != 0 || size == 0 {
        return Vec::new();
    }
    let mut out = vec![0 as Id; size as usize / size_of::<Id>()];
    // SAFETY: the buffer holds `size` bytes.
    let status = unsafe {
        AudioObjectGetPropertyData(
            id,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::new(out.as_mut_ptr().cast()).unwrap_or(NonNull::dangling()),
        )
    };
    out.truncate(size as usize / size_of::<Id>());
    if status == 0 { out } else { Vec::new() }
}

fn get_string(id: Id, selector: u32) -> Option<String> {
    let mut addr = address(selector, kAudioObjectPropertyScopeGlobal);
    let mut size = size_of::<*const CFString>() as u32;
    let mut value: *const CFString = std::ptr::null();
    // SAFETY: the property is a CFString that we receive with one reference.
    let status = unsafe {
        AudioObjectGetPropertyData(
            id,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    if status != 0 {
        return None;
    }
    let p = NonNull::new(value.cast_mut())?;
    // SAFETY: ownership of the reference moves into CFRetained.
    let s: CFRetained<CFString> = unsafe { CFRetained::from_raw(p) };
    Some(s.to_string())
}

pub fn uid(device: Id) -> Option<String> {
    get_string(device, kAudioDevicePropertyDeviceUID)
}

pub fn default_output() -> Option<Id> {
    get::<Id>(SYSTEM, kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyScopeGlobal)
        .filter(|id| *id != kAudioObjectUnknown)
}

pub fn sample_rate(device: Id) -> Option<f64> {
    get::<f64>(device, kAudioDevicePropertyNominalSampleRate, kAudioObjectPropertyScopeGlobal)
        .filter(|r| *r > 1000.0)
}

/// Core Audio's object for this process, needed to leave our own sound out of
/// the capture.
pub fn own_process() -> Option<Id> {
    let mut addr =
        address(kAudioHardwarePropertyTranslatePIDToProcessObject, kAudioObjectPropertyScopeGlobal);
    let pid = std::process::id() as libc::pid_t;
    let mut out: Id = 0;
    let mut size = size_of::<Id>() as u32;
    // SAFETY: qualifier and output match the documented types.
    let status = unsafe {
        AudioObjectGetPropertyData(
            SYSTEM,
            NonNull::from(&mut addr),
            size_of::<libc::pid_t>() as u32,
            (&raw const pid).cast::<c_void>(),
            NonNull::from(&mut size),
            NonNull::from(&mut out).cast(),
        )
    };
    (status == 0 && out != kAudioObjectUnknown).then_some(out)
}

pub fn devices() -> Vec<Device> {
    let default = default_output();
    get_list(SYSTEM, kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal)
        .into_iter()
        .filter(|d| !get_list(*d, kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput).is_empty())
        .filter_map(|d| {
            let id = uid(d)?;
            if id.starts_with(AGGREGATE_PREFIX) {
                return None;
            }
            let name = get_string(d, kAudioObjectPropertyName).unwrap_or_else(|| id.clone());
            Some(Device { id, name, is_default: default == Some(d) })
        })
        .collect()
}

pub const AGGREGATE_PREFIX: &str = "io.github.adityatummuri.eqaura.aggregate";

/// A tap on all system sound except our own process.
pub struct Tap {
    pub id: Id,
    pub uuid: String,
}

impl Tap {
    pub fn new(exclude: Option<Id>, mute: bool) -> Result<Self, String> {
        let list: Vec<Retained<NSNumber>> = exclude.into_iter().map(NSNumber::new_u32).collect();
        let list = NSArray::from_retained_slice(&list);
        let mut id: Id = 0;
        // SAFETY: plain Objective-C calls on an object we own.
        let (status, uuid) = unsafe {
            let d = CATapDescription::initStereoGlobalTapButExcludeProcesses(
                CATapDescription::alloc(),
                &list,
            );
            d.setName(&NSString::from_str("EQaura"));
            d.setPrivate(true);
            d.setMuteBehavior(if mute {
                CATapMuteBehavior::MutedWhenTapped
            } else {
                CATapMuteBehavior::Unmuted
            });
            let uuid = d.UUID().UUIDString().to_string();
            (AudioHardwareCreateProcessTap(Some(&d), &mut id), uuid)
        };
        check("Capturing system sound", status)?;
        Ok(Self { id, uuid })
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        // SAFETY: the id came from a successful create call.
        unsafe { AudioHardwareDestroyProcessTap(self.id) };
    }
}

/// A private device that joins the real output with the tap on one clock.
pub struct Aggregate {
    pub id: Id,
}

fn key(k: &CStr) -> Retained<NSString> {
    NSString::from_str(&k.to_string_lossy())
}

fn object<T: objc2::Message>(v: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an AnyObject.
    unsafe { Retained::cast_unchecked(v) }
}

fn dictionary(items: Vec<(&CStr, Retained<AnyObject>)>) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<Retained<NSString>> = items.iter().map(|(k, _)| key(k)).collect();
    let refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
    let values: Vec<Retained<AnyObject>> = items.into_iter().map(|(_, v)| v).collect();
    NSDictionary::from_retained_objects(&refs, &values)
}

impl Aggregate {
    pub fn new(output_uid: &str, tap_uuid: &str) -> Result<Self, String> {
        let text = |s: &str| object(NSString::from_str(s));
        let flag = |b: bool| object(NSNumber::new_bool(b));
        let sub = dictionary(vec![(kAudioSubDeviceUIDKey, text(output_uid))]);
        let tap = dictionary(vec![
            (kAudioSubTapUIDKey, text(tap_uuid)),
            (kAudioSubTapDriftCompensationKey, flag(true)),
        ]);
        let unique = format!("{AGGREGATE_PREFIX}.{}.{tap_uuid}", std::process::id());
        let dict = dictionary(vec![
            (kAudioAggregateDeviceUIDKey, text(&unique)),
            (kAudioAggregateDeviceNameKey, text("EQaura")),
            (kAudioAggregateDeviceMainSubDeviceKey, text(output_uid)),
            (kAudioAggregateDeviceIsPrivateKey, flag(true)),
            (kAudioAggregateDeviceIsStackedKey, flag(false)),
            (kAudioAggregateDeviceTapAutoStartKey, flag(true)),
            (
                kAudioAggregateDeviceSubDeviceListKey,
                object(NSArray::from_retained_slice(&[sub])),
            ),
            (kAudioAggregateDeviceTapListKey, object(NSArray::from_retained_slice(&[tap]))),
        ]);
        let mut id: Id = 0;
        // SAFETY: NSDictionary and CFDictionary are toll-free bridged.
        let status = unsafe {
            let cf = &*(Retained::as_ptr(&dict).cast::<CFDictionary>());
            AudioHardwareCreateAggregateDevice(cf, NonNull::from(&mut id))
        };
        check("Creating the output device", status)?;
        Ok(Self { id })
    }
}

impl Drop for Aggregate {
    fn drop(&mut self) {
        // SAFETY: the id came from a successful create call.
        unsafe { AudioHardwareDestroyAggregateDevice(self.id) };
    }
}

/// Uses only the last input stream (the tap), so a headset's microphone stays
/// off and Bluetooth keeps its high-quality mode.
pub fn use_only_tap(device: Id, proc_id: AudioDeviceIOProcID) {
    let streams = get_list(device, kAudioDevicePropertyStreams, kAudioObjectPropertyScopeInput).len();
    if streams < 2 {
        return;
    }
    let Some(proc_ptr) = proc_id else { return };
    // Variable-length C struct: pointer, count, then one u32 per stream.
    let mut words = vec![0u64; 2 + streams.div_ceil(2)];
    let base = words.as_mut_ptr().cast::<u8>();
    let size = (size_of::<*const c_void>() + 4 + 4 * streams) as u32;
    // SAFETY: `words` is larger than `size` and aligned for the pointer.
    unsafe {
        base.cast::<*const c_void>().write(proc_ptr as *const c_void);
        let counts = base.add(size_of::<*const c_void>()).cast::<u32>();
        counts.write(streams as u32);
        for i in 0..streams {
            counts.add(1 + i).write(u32::from(i + 1 == streams));
        }
        let mut addr = address(kAudioDevicePropertyIOProcStreamUsage, kAudioObjectPropertyScopeInput);
        if let Some(data) = NonNull::new(base.cast::<c_void>()) {
            // Failure only means the microphone stream stays open.
            AudioObjectSetPropertyData(device, NonNull::from(&mut addr), 0, std::ptr::null(), size, data);
        }
    }
}
