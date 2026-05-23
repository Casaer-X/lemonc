//! Built-in runtime library for Lemon native executables
//!
//! Provides the runtime functions that Lemon programs depend on:
//! - Memory management (malloc, free)
//! - Array operations (create, push, get, set, length)
//! - Map operations (create, get, put, contains, remove, keys)
//! - String operations (concat, length, equals, join, split)
//! - I/O operations (print, println, readLine)
//! - Type conversion (intToString, parseFloat, etc.)
//! - VM interop (for hybrid mode)

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

// === Memory Management ===

/// Lemon object header - all heap objects start with this
#[repr(C)]
pub struct LemonObject {
    pub type_id: u32,
    pub ref_count: u32,
    pub size: u64,
}

/// Lemon array structure
#[repr(C)]
pub struct LemonArray {
    pub length: i64,
    pub capacity: i64,
    pub data: *mut *mut LemonObject,
}

/// Lemon map entry
#[repr(C)]
pub struct LemonMapEntry {
    pub key: *mut LemonObject,
    pub value: *mut LemonObject,
}

/// Lemon map structure
#[repr(C)]
pub struct LemonMap {
    pub size: i64,
    pub capacity: i64,
    pub entries: *mut LemonMapEntry,
}

// === Array Runtime Functions ===

/// Create a new LemonArray with given initial capacity
#[no_mangle]
pub unsafe extern "C" fn LemonArray_new(capacity: i64) -> *mut LemonArray {
    let cap = if capacity <= 0 { 8 } else { capacity };
    let data = libc_malloc(cap * 8) as *mut *mut LemonObject;
    let arr = Box::new(LemonArray {
        length: 0,
        capacity: cap,
        data,
    });
    Box::into_raw(arr)
}

/// Get array length
#[no_mangle]
pub unsafe extern "C" fn LemonArray_length(arr: *mut LemonArray) -> i64 {
    if arr.is_null() { return 0; }
    (*arr).length
}

/// Get array element at index
#[no_mangle]
pub unsafe extern "C" fn LemonArray_get(arr: *mut LemonArray, index: i64) -> *mut LemonObject {
    if arr.is_null() || index < 0 || index >= (*arr).length {
        return std::ptr::null_mut();
    }
    let data = (*arr).data;
    *data.offset(index as isize)
}

/// Set array element at index
#[no_mangle]
pub unsafe extern "C" fn LemonArray_set(arr: *mut LemonArray, index: i64, value: *mut LemonObject) {
    if arr.is_null() || index < 0 || index >= (*arr).length { return; }
    let data = (*arr).data;
    *data.offset(index as isize) = value;
}

/// Push element to end of array
#[no_mangle]
pub unsafe extern "C" fn LemonArray_push(arr: *mut LemonArray, value: *mut LemonObject) {
    if arr.is_null() { return; }
    if (*arr).length >= (*arr).capacity {
        // Grow: double capacity
        let new_cap = (*arr).capacity * 2;
        let new_data = libc_malloc(new_cap * 8) as *mut *mut LemonObject;
        std::ptr::copy_nonoverlapping((*arr).data, new_data, (*arr).length as usize);
        libc_free((*arr).data as *mut std::ffi::c_void);
        (*arr).data = new_data;
        (*arr).capacity = new_cap;
    }
    let data = (*arr).data;
    *data.offset((*arr).length as isize) = value;
    (*arr).length += 1;
}

// === String Runtime Functions ===

/// String length
#[no_mangle]
pub unsafe extern "C" fn LemonString_length(s: *const c_char) -> i64 {
    if s.is_null() { return 0; }
    CStr::from_ptr(s).to_bytes().len() as i64
}

/// String concatenation
#[no_mangle]
pub unsafe extern "C" fn LemonString_concat(a: *const c_char, b: *const c_char) -> *mut c_char {
    if a.is_null() && b.is_null() {
        return CString::new("").unwrap().into_raw();
    }
    if a.is_null() { return CString::from(CStr::from_ptr(b)).into_raw(); }
    if b.is_null() { return CString::from(CStr::from_ptr(a)).into_raw(); }

    let sa = CStr::from_ptr(a).to_bytes();
    let sb = CStr::from_ptr(b).to_bytes();
    let mut result = Vec::with_capacity(sa.len() + sb.len());
    result.extend_from_slice(sa);
    result.extend_from_slice(sb);
    CString::new(result).unwrap().into_raw()
}

/// String equality
#[no_mangle]
pub unsafe extern "C" fn LemonString_equals(a: *const c_char, b: *const c_char) -> i64 {
    if a.is_null() || b.is_null() { return 0; }
    if CStr::from_ptr(a) == CStr::from_ptr(b) { 1 } else { 0 }
}

/// Int to string
#[no_mangle]
pub extern "C" fn LemonString_intToString(v: i64) -> *mut c_char {
    CString::new(v.to_string()).unwrap().into_raw()
}

/// Float to string
#[no_mangle]
pub extern "C" fn LemonString_floatToString(v: f64) -> *mut c_char {
    CString::new(format!("{:.6}", v)).unwrap().into_raw()
}

/// String join
#[no_mangle]
pub unsafe extern "C" fn LemonString_join(arr: *mut LemonArray, sep: *const c_char) -> *mut c_char {
    if arr.is_null() { return CString::new("").unwrap().into_raw(); }
    let sep_str = if sep.is_null() { "" } else { CStr::from_ptr(sep).to_str().unwrap_or("") };
    let mut parts = Vec::new();
    for i in 0..(*arr).length {
        let elem = LemonArray_get(arr, i);
        if !elem.is_null() {
            // Assume element is a string pointer
            let s = CStr::from_ptr(elem as *const c_char).to_str().unwrap_or("").to_string();
            parts.push(s);
        }
    }
    CString::new(parts.join(sep_str)).unwrap().into_raw()
}

// === I/O Runtime Functions ===

/// Print a string
#[no_mangle]
pub unsafe extern "C" fn LemonIO_print(s: *const c_char) {
    if s.is_null() { return; }
    let bytes = CStr::from_ptr(s).to_bytes();
    let _ = std::io::Write::write_all(&mut std::io::stdout(), bytes);
}

/// Print a string with newline
#[no_mangle]
pub unsafe extern "C" fn LemonIO_println(s: *const c_char) {
    LemonIO_print(s);
    println!();
}

/// Read a line from stdin
#[no_mangle]
pub extern "C" fn LemonIO_readLine() -> *mut c_char {
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_ok() {
        let trimmed = input.trim_end_matches('\n').trim_end_matches('\r');
        CString::new(trimmed).unwrap().into_raw()
    } else {
        CString::new("").unwrap().into_raw()
    }
}

// === VM Interop (Hybrid Mode) ===

/// Initialize the embedded VM from the .lmb section
/// Called by the hybrid executable's startup code
#[no_mangle]
pub unsafe extern "C" fn LemonVM_init(lmb_data: *const u8, lmb_size: u32) -> *mut std::ffi::c_void {
    use crate::jit::vm::LeVM;
    match LeVM::from_bytes(std::slice::from_raw_parts(lmb_data, lmb_size as usize)) {
        Ok(vm) => Box::into_raw(Box::new(vm)) as *mut std::ffi::c_void,
        Err(_) => std::ptr::null_mut(),
    }
}

/// Call a VM function by name
#[no_mangle]
pub unsafe extern "C" fn LemonVM_callByName(
    vm: *mut std::ffi::c_void,
    name: *const c_char,
) -> i64 {
    if vm.is_null() || name.is_null() { return 0; }
    let vm = &mut *(vm as *mut crate::jit::vm::LeVM);
    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    match vm.call_by_name(name_str, vec![crate::jit::vm::VMValue::Null]) {
        Ok(result) => {
            match result {
                crate::jit::vm::VMValue::Int(v) => v,
                _ => 0,
            }
        }
        Err(_) => 0,
    }
}

/// Destroy the embedded VM
#[no_mangle]
pub unsafe extern "C" fn LemonVM_destroy(vm: *mut std::ffi::c_void) {
    if vm.is_null() { return; }
    drop(Box::from_raw(vm as *mut crate::jit::vm::LeVM));
}

// === Memory Allocation Helpers ===

fn libc_malloc(size: i64) -> *mut std::ffi::c_void {
    let layout = std::alloc::Layout::from_size_align(size as usize, 8).unwrap();
    unsafe { std::alloc::alloc(layout) as *mut std::ffi::c_void }
}

fn libc_free(ptr: *mut std::ffi::c_void) {
    if ptr.is_null() { return; }
    // Note: In production, we'd need to track the layout for proper deallocation
    // For now, this is a simplified implementation
    unsafe {
        let layout = std::alloc::Layout::from_size_align(0, 8).unwrap();
        std::alloc::dealloc(ptr as *mut u8, layout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_concat() {
        let a = CString::new("hello").unwrap();
        let b = CString::new(" world").unwrap();
        let result = unsafe { LemonString_concat(a.as_ptr(), b.as_ptr()) };
        let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(result_str, "hello world");
        unsafe { libc_free(result as *mut std::ffi::c_void); }
    }

    #[test]
    fn test_string_length() {
        let s = CString::new("hello").unwrap();
        let len = unsafe { LemonString_length(s.as_ptr()) };
        assert_eq!(len, 5);
    }

    #[test]
    fn test_int_to_string() {
        let result = LemonString_intToString(42);
        let s = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
        assert_eq!(s, "42");
        unsafe { libc_free(result as *mut std::ffi::c_void); }
    }

    #[test]
    fn test_array_new() {
        let arr = unsafe { LemonArray_new(4) };
        assert!(!arr.is_null());
        let len = unsafe { LemonArray_length(arr) };
        assert_eq!(len, 0);
    }
}
