const RESPONSE: &[u8] = br#"{"ok":true,"message":"native library isolated","component":"example.native-library-echo"}"#;

#[no_mangle]
pub extern "system" fn nexora_component_abi_v1() -> u32 {
    1
}

/// The host owns all pointers and the response buffer. This deliberately
/// minimal example proves the ABI boundary without accessing files or state.
#[no_mangle]
pub unsafe extern "system" fn nexora_component_invoke_v1(
    _request_ptr: *const u8,
    _request_len: usize,
    response_ptr: *mut u8,
    response_capacity: usize,
) -> i64 {
    if response_ptr.is_null() || response_capacity < RESPONSE.len() {
        return -1;
    }
    std::ptr::copy_nonoverlapping(RESPONSE.as_ptr(), response_ptr, RESPONSE.len());
    RESPONSE.len() as i64
}
