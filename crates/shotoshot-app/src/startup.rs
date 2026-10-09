use windows::{
    Win32::{Foundation::*, System::Registry::*},
    core::*,
};
const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
pub fn enabled() -> Result<bool> {
    unsafe {
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            RUN,
            w!("Shotoshot"),
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        );
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(false);
        }
        status.ok()?;
        Ok(true)
    }
}
pub fn set_enabled(enabled: bool) -> Result<()> {
    unsafe {
        let path = std::env::current_exe().map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        let mut key = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()?;
        let result = if enabled {
            let mut command = vec![b'"' as u16];
            command.extend(crate::wide(&path).into_iter().take_while(|v| *v != 0));
            command.extend([b'"' as u16, 0]);
            let bytes: Vec<u8> = command.iter().flat_map(|v| v.to_le_bytes()).collect();
            RegSetValueExW(key, w!("Shotoshot"), None, REG_SZ, Some(&bytes)).ok()
        } else {
            let status = RegDeleteValueW(key, w!("Shotoshot"));
            if status == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                status.ok()
            }
        };
        let _ = RegCloseKey(key);
        result
    }
}
