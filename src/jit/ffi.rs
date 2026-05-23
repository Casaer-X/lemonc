//! C FFI interface for LeVM
//!
//! Provides C-compatible functions for native code to create, use, and destroy
//! an embedded Lemon VM instance. These are the primary entry points for the
//! AOT↔VM interop layer.

use crate::jit::vm::{LeVM, LeNativeFunc, VMValue};
use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;

/// Opaque handle for LeVM used in C FFI
pub type LeVMHandle = LeVM;

/// Create a LeVM instance from embedded .lmb bytecode data.
///
/// # Safety
/// - `lmb_data` must point to valid .lmb bytecode of `lmb_size` bytes
/// - The returned pointer must be freed with `le_vm_destroy`
#[no_mangle]
pub unsafe extern "C" fn le_vm_create(lmb_data: *const u8, lmb_size: u32) -> *mut LeVMHandle {
    if lmb_data.is_null() || lmb_size == 0 {
        return ptr::null_mut();
    }

    let data = std::slice::from_raw_parts(lmb_data, lmb_size as usize);
    match LeVM::from_bytes(data) {
        Ok(vm) => Box::into_raw(Box::new(vm)),
        Err(_) => ptr::null_mut(),
    }
}

/// Destroy a LeVM instance and free all associated memory.
///
/// # Safety
/// - `vm` must be a valid pointer returned by `le_vm_create`
/// - `vm` must not be used after calling this function
#[no_mangle]
pub unsafe extern "C" fn le_vm_destroy(vm: *mut LeVMHandle) {
    if !vm.is_null() {
        drop(Box::from_raw(vm));
    }
}

/// Call a function in the VM by its index.
///
/// Arguments are passed as an array of raw pointers. Each argument is
/// interpreted based on the function's expected parameter types:
/// - Int parameters: the pointer value itself is used as i64
/// - Float parameters: the pointer is cast to f64
/// - String/Object parameters: the pointer is stored as VMValue::Ptr
///
/// Returns a raw pointer representing the result value, or null on error.
/// The caller must not free the returned pointer.
///
/// # Safety
/// - `vm` must be a valid LeVM pointer
/// - `args` must point to an array of `argc` pointers (or null if argc=0)
#[no_mangle]
pub unsafe extern "C" fn le_vm_call(
    vm: *mut LeVMHandle,
    func_idx: u32,
    args: *mut *mut std::ffi::c_void,
    argc: i32,
) -> *mut std::ffi::c_void {
    if vm.is_null() {
        return ptr::null_mut();
    }

    let vm = &mut *vm;
    let vm_args = if args.is_null() || argc <= 0 {
        Vec::new()
    } else {
        let args_slice = std::slice::from_raw_parts(args, argc as usize);
        args_slice
            .iter()
            .map(|&p| {
                if p.is_null() {
                    VMValue::Null
                } else {
                    VMValue::from_ptr(p)
                }
            })
            .collect()
    };

    match vm.call(func_idx, vm_args) {
        Ok(result) => match result {
            VMValue::Int(v) => v as *mut std::ffi::c_void,
            VMValue::Ptr(p) => p,
            VMValue::Null => ptr::null_mut(),
            VMValue::Bool(b) => (b as usize) as *mut std::ffi::c_void,
            _ => {
                // For String, Float, Array, Object: box the value and return pointer
                // Caller should use le_vm_result_to_int / le_vm_result_to_float for extraction
                Box::into_raw(Box::new(result)) as *mut std::ffi::c_void
            }
        },
        Err(_) => ptr::null_mut(),
    }
}

/// Call a function in the VM by its name.
///
/// # Safety
/// - `vm` must be a valid LeVM pointer
/// - `name` must be a null-terminated C string
/// - `args` must point to an array of `argc` pointers (or null if argc=0)
#[no_mangle]
pub unsafe extern "C" fn le_vm_call_by_name(
    vm: *mut LeVMHandle,
    name: *const c_char,
    args: *mut *mut std::ffi::c_void,
    argc: i32,
) -> *mut std::ffi::c_void {
    if vm.is_null() || name.is_null() {
        return ptr::null_mut();
    }

    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    let vm = &mut *vm;
    let vm_args = if args.is_null() || argc <= 0 {
        Vec::new()
    } else {
        let args_slice = std::slice::from_raw_parts(args, argc as usize);
        args_slice
            .iter()
            .map(|&p| {
                if p.is_null() {
                    VMValue::Null
                } else {
                    VMValue::from_ptr(p)
                }
            })
            .collect()
    };

    match vm.call_by_name(name_str, vm_args) {
        Ok(result) => match result {
            VMValue::Int(v) => v as *mut std::ffi::c_void,
            VMValue::Ptr(p) => p,
            VMValue::Null => ptr::null_mut(),
            VMValue::Bool(b) => (b as usize) as *mut std::ffi::c_void,
            _ => Box::into_raw(Box::new(result)) as *mut std::ffi::c_void,
        },
        Err(_) => ptr::null_mut(),
    }
}

/// Register a native function that can be called from VM bytecode.
///
/// # Safety
/// - `vm` must be a valid LeVM pointer
/// - `name` must be a null-terminated C string
/// - `func` must be a valid function pointer that remains valid for the
///   lifetime of the VM
#[no_mangle]
pub unsafe extern "C" fn le_vm_register_native(
    vm: *mut LeVMHandle,
    name: *const c_char,
    func: LeNativeFunc,
) {
    if vm.is_null() || name.is_null() {
        return;
    }

    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return,
    };

    let vm = &mut *vm;
    vm.register_native(name_str, func);
}

/// Find a function index by name.
///
/// Returns the function index, or -1 if not found.
///
/// # Safety
/// - `vm` must be a valid LeVM pointer
/// - `name` must be a null-terminated C string
#[no_mangle]
pub unsafe extern "C" fn le_vm_find_function(
    vm: *mut LeVMHandle,
    name: *const c_char,
) -> i32 {
    if vm.is_null() || name.is_null() {
        return -1;
    }

    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let vm = &*vm;
    match vm.find_function(name_str) {
        Some(idx) => idx as i32,
        None => -1,
    }
}

/// Set the JIT compilation threshold.
///
/// When a function's execution count exceeds this threshold, it becomes
/// a candidate for JIT compilation. Set to 0 to disable.
///
/// # Safety
/// - `vm` must be a valid LeVM pointer
#[no_mangle]
pub unsafe extern "C" fn le_vm_set_jit_threshold(vm: *mut LeVMHandle, threshold: u64) {
    if vm.is_null() {
        return;
    }
    let vm = &mut *vm;
    vm.set_jit_threshold(threshold);
}

/// Reset the VM state (clear stack, frames, globals) but keep the module
/// and registered native functions.
///
/// # Safety
/// - `vm` must be a valid LeVM pointer
#[no_mangle]
pub unsafe extern "C" fn le_vm_reset(vm: *mut LeVMHandle) {
    if vm.is_null() {
        return;
    }
    let vm = &mut *vm;
    vm.reset();
}

/// Extract an integer result from a VMValue pointer.
///
/// # Safety
/// - `result` must be a valid pointer returned by le_vm_call
#[no_mangle]
pub unsafe extern "C" fn le_vm_result_to_int(result: *mut std::ffi::c_void) -> i64 {
    if result.is_null() {
        return 0;
    }
    // Check if it's a boxed VMValue
    let vm_val = &*(result as *const VMValue);
    vm_val.as_int()
}

/// Extract a float result from a VMValue pointer.
///
/// # Safety
/// - `result` must be a valid pointer to a VMValue returned by le_vm_call
#[no_mangle]
pub unsafe extern "C" fn le_vm_result_to_float(result: *mut std::ffi::c_void) -> f64 {
    if result.is_null() {
        return 0.0;
    }
    let vm_val = &*(result as *const VMValue);
    vm_val.as_float()
}

/// Free a VMValue pointer returned by le_vm_call.
///
/// # Safety
/// - `result` must be a pointer returned by le_vm_call that was boxed
///   (i.e., for String/Float/Array/Object results)
#[no_mangle]
pub unsafe extern "C" fn le_vm_result_free(result: *mut std::ffi::c_void) {
    if result.is_null() {
        return;
    }
    drop(Box::from_raw(result as *mut VMValue));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jit::bytecode::*;
    use crate::jit::serialize::write_module;

    fn make_test_module() -> BytecodeModule {
        let mut module = BytecodeModule::new();
        // Add a simple function that returns 42
        module.functions.push(BytecodeFunction {
            name: "test_func".to_string(),
            params: vec![],
            locals: 0,
            code: vec![Bytecode::PushConst(42), Bytecode::Return],
            is_static: true,
            class_name: None,
        });
        module.entry_point = 0;
        module
    }

    #[test]
    fn test_ffi_create_destroy() {
        let module = make_test_module();
        let mut lmb_data = Vec::new();
        write_module(&mut lmb_data, &module).unwrap();

        let vm = unsafe { le_vm_create(lmb_data.as_ptr(), lmb_data.len() as u32) };
        assert!(!vm.is_null());
        unsafe { le_vm_destroy(vm) };
    }

    #[test]
    fn test_ffi_create_null_data() {
        let vm = unsafe { le_vm_create(std::ptr::null(), 0) };
        assert!(vm.is_null());
    }

    #[test]
    fn test_ffi_find_function() {
        let module = make_test_module();
        let mut lmb_data = Vec::new();
        write_module(&mut lmb_data, &module).unwrap();

        let vm = unsafe { le_vm_create(lmb_data.as_ptr(), lmb_data.len() as u32) };
        assert!(!vm.is_null());

        let name = b"test_func\0";
        let idx = unsafe { le_vm_find_function(vm, name.as_ptr() as *const c_char) };
        assert_eq!(idx, 0);

        let name2 = b"nonexistent\0";
        let idx2 = unsafe { le_vm_find_function(vm, name2.as_ptr() as *const c_char) };
        assert_eq!(idx2, -1);

        unsafe { le_vm_destroy(vm) };
    }

    #[test]
    fn test_ffi_call() {
        let module = make_test_module();
        let mut lmb_data = Vec::new();
        write_module(&mut lmb_data, &module).unwrap();

        let vm = unsafe { le_vm_create(lmb_data.as_ptr(), lmb_data.len() as u32) };
        assert!(!vm.is_null());

        let result = unsafe { le_vm_call(vm, 0, std::ptr::null_mut(), 0) };
        // The function returns Int(42), so result is 42 as a pointer
        assert_eq!(result as i64, 42);

        unsafe { le_vm_destroy(vm) };
    }
}
