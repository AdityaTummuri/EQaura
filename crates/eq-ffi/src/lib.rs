//! The Android app's door to the core. Every call takes JSON and returns JSON,
//! and keeps no state, so nothing here can get out of step with the app.

pub mod api;
pub mod bands;

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jdouble, jfloatArray, jstring};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// `Core.call(method, args)`. Never throws: problems come back as `error`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_adityatummuri_eqaura_Core_call<'l>(
    mut env: JNIEnv<'l>,
    _: JClass<'l>,
    method: JString<'l>,
    args: JString<'l>,
) -> jstring {
    // A panic must not unwind into Java.
    let out = catch_unwind(AssertUnwindSafe(|| {
        let method: String = env.get_string(&method).map(Into::into).unwrap_or_default();
        let args: String = env.get_string(&args).map(Into::into).unwrap_or_default();
        api::call(&method, &args)
    }))
    .unwrap_or_else(|_| r#"{"error":"Something went wrong inside EQaura."}"#.into());
    env.new_string(out).map_or(std::ptr::null_mut(), |s| s.into_raw())
}

/// `Core.tone(left, freq, db, rate)`: one hearing test tone, interleaved stereo.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_adityatummuri_eqaura_Core_tone<'l>(
    env: JNIEnv<'l>,
    _: JClass<'l>,
    left: jboolean,
    freq: jdouble,
    db: jdouble,
    rate: jdouble,
) -> jfloatArray {
    let samples = catch_unwind(|| api::tone(left != 0, freq, db, rate)).unwrap_or_default();
    let Ok(array) = env.new_float_array(samples.len() as i32) else { return std::ptr::null_mut() };
    if env.set_float_array_region(&array, 0, &samples).is_err() {
        return std::ptr::null_mut();
    }
    array.into_raw()
}
