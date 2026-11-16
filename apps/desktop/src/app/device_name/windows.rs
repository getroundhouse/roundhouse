use windows_sys::Win32::System::WindowsProgramming::GetComputerNameW;

pub(super) fn computer_name() -> String {
    let mut name = [0u16; 256];
    let mut length = 256;
    // SAFETY: the writable buffer has the length passed to Windows and lives through the call.
    let found = unsafe { GetComputerNameW(name.as_mut_ptr(), &raw mut length) };
    if found == 0 { String::new() } else { String::from_utf16_lossy(&name[..length as usize]) }
}
