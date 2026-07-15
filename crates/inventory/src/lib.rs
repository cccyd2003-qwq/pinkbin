//! Installed-software inventory + leftover scanner.
//!
//! `list_installed_apps` reads the Windows registry Uninstall keys to enumerate
//! all installed software (name, version, publisher, install path, uninstall
//! string). `find_leftovers` scans common app-data locations for remnants
//! after a software has been uninstalled.
//!
//! On non-Windows, these functions return empty vecs — the crate still
//! compiles for cross-compilation but the registry logic is cfg(windows) only.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// A single installed application, as read from the registry.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledApp {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_size_mb: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uninstall_string: Option<String>,
    pub is_per_user: bool,
}

/// Scan result for leftover files after an app uninstall.
#[derive(Debug, Serialize)]
pub struct LeftoverResult {
    pub app_name: String,
    pub found_paths: Vec<LeftoverPath>,
    pub total_size_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct LeftoverPath {
    pub path: String,
    pub size_bytes: u64,
    pub is_dir: bool,
}

/// Enumerate all installed applications from the Windows registry.
///
/// Reads three Uninstall keys:
///   - HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*
///   - HKLM\SOFTWARE\WOW6432Node\...\Uninstall\*  (32-bit on 64-bit OS)
///   - HKCU\SOFTWARE\...\Uninstall\*              (per-user installs)
///
/// On non-Windows, returns empty vec.
pub fn list_installed_apps() -> Vec<InstalledApp> {
    #[cfg(windows)]
    return list_installed_apps_windows();

    #[cfg(not(windows))]
    {
        tracing::warn!("list_installed_apps: not on Windows, returning empty");
        Vec::new()
    }
}

/// Find leftover files/directories for an app that has been uninstalled.
///
/// Scans common app-data locations:
///   - %APPDATA%/<app_name>
///   - %LOCALAPPDATA%/<app_name>
///   - %PROGRAMDATA%/<app_name>
///
/// Matching is case-insensitive substring on the app name.
pub fn find_leftovers(app_name: &str) -> LeftoverResult {
    let search_dirs = get_app_data_dirs();
    let lower_name = app_name.to_lowercase();
    let mut found_paths = Vec::new();
    let mut total_size = 0u64;

    for base in &search_dirs {
        if !base.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(base)
            .max_depth(1)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let entry_name = entry.file_name().to_string_lossy().to_lowercase();
            if entry_name.contains(&lower_name) {
                let path = entry.path().to_path_buf();
                let is_dir = entry.file_type().is_dir();
                let size = if is_dir {
                    dir_size(&path)
                } else {
                    entry.metadata().map(|m| m.len()).unwrap_or(0)
                };
                total_size += size;
                found_paths.push(LeftoverPath {
                    path: path.to_string_lossy().replace('\\', "/"),
                    size_bytes: size,
                    is_dir,
                });
            }
        }
    }

    LeftoverResult {
        app_name: app_name.to_string(),
        found_paths,
        total_size_bytes: total_size,
    }
}

/// Find an installed app by fuzzy name match (case-insensitive substring).
pub fn find_by_name<'a>(apps: &'a [InstalledApp], query: &str) -> Vec<&'a InstalledApp> {
    let q = query.to_lowercase();
    apps.iter()
        .filter(|a| a.name.to_lowercase().contains(&q))
        .collect()
}

// ─────────────────────────── helpers ───────────────────────────

fn get_app_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for var in &["APPDATA", "LOCALAPPDATA", "PROGRAMDATA"] {
        if let Ok(val) = std::env::var(var) {
            dirs.push(PathBuf::from(val));
        }
    }
    dirs
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Ok(md) = entry.metadata() {
                total = total.saturating_add(md.len());
            }
        }
    }
    total
}

// ─────────────────────────── Windows registry impl ───────────────────────────

#[cfg(windows)]
fn list_installed_apps_windows() -> Vec<InstalledApp> {
    use windows_sys::Win32::System::Registry::*;

    let mut apps = Vec::new();

    // Three Uninstall keys to scan
    const KEYS: &[(HKEY, &str, bool)] = &[
        (HKEY_LOCAL_MACHINE, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall", false),
        (HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall", false),
        (HKEY_CURRENT_USER, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall", true),
    ];

    for &(root, subkey, is_per_user) in KEYS {
        let mut hkey: HKEY = std::ptr::null_mut();
        let subkey_wide: Vec<u16> = subkey
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let status = unsafe {
            RegOpenKeyExW(root, subkey_wide.as_ptr(), 0, KEY_READ, &mut hkey)
        };
        if status != 0 {
            continue;
        }

        let mut index = 0u32;
        loop {
            let name_cap = 255u32;
            let mut name_buf = [0u16; 256];
            let status = unsafe {
                RegEnumKeyW(hkey, index, name_buf.as_mut_ptr(), name_cap)
            };
            if status != 0 {
                break; // ERROR_NO_MORE_ITEMS or error
            }
            // RegEnumKeyW does not return the written length; compute it from NUL.
            let written = name_buf.iter().position(|&c| c == 0).unwrap_or(name_buf.len());
            index += 1;

            let subpath = format!("{}\\{}", subkey, String::from_utf16_lossy(&name_buf[..written]));
            let subpath_wide: Vec<u16> = subpath
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let mut sub_hkey: HKEY = std::ptr::null_mut();
            let status = unsafe {
                RegOpenKeyExW(root, subpath_wide.as_ptr(), 0, KEY_READ, &mut sub_hkey)
            };
            if status != 0 {
                continue;
            }

            let app = read_app_from_registry(sub_hkey, is_per_user);
            unsafe { RegCloseKey(sub_hkey); }

            if let Some(a) = app {
                if !a.name.is_empty() {
                    apps.push(a);
                }
            }
        }

        unsafe { RegCloseKey(hkey); }
    }

    // Deduplicate by name (same app may appear in multiple keys)
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));

    apps
}

#[cfg(windows)]
fn read_app_from_registry(hkey: windows_sys::Win32::System::Registry::HKEY, is_per_user: bool) -> Option<InstalledApp> {
    let name = read_reg_string(hkey, "DisplayName")?;
    let version = read_reg_string(hkey, "DisplayVersion");
    let publisher = read_reg_string(hkey, "Publisher");
    let install_path = read_reg_string(hkey, "InstallLocation");
    let install_date = read_reg_string(hkey, "InstallDate");
    let uninstall_string = read_reg_string(hkey, "UninstallString");
    let estimated_size_mb = read_reg_dword(hkey, "EstimatedSize")
        .map(|v| (v as u64) / 1024); // EstimatedSize is in KB

    Some(InstalledApp {
        name,
        version,
        publisher,
        install_path,
        install_date,
        estimated_size_mb,
        uninstall_string,
        is_per_user,
    })
}

#[cfg(windows)]
fn read_reg_string(hkey: windows_sys::Win32::System::Registry::HKEY, name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::*;
    let name_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut buf = [0u8; 2048];
    let mut buf_len = buf.len() as u32;
    let mut reg_type = 0u32;

    let status = unsafe {
        RegQueryValueExW(
            hkey,
            name_wide.as_ptr(),
            std::ptr::null(),
            &mut reg_type,
            buf.as_mut_ptr(),
            &mut buf_len,
        )
    };
    if status != 0 || reg_type != REG_SZ {
        return None;
    }

    let chars: Vec<u16> = (0..buf_len as usize / 2)
        .map(|i| u16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]))
        .collect();
    let s = String::from_utf16_lossy(&chars);
    let s = s.trim_end_matches('\0').to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(windows)]
fn read_reg_dword(hkey: windows_sys::Win32::System::Registry::HKEY, name: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::*;
    let name_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut val: u32 = 0;
    let mut buf_len = 4u32;
    let mut reg_type = 0u32;

    let status = unsafe {
        RegQueryValueExW(
            hkey,
            name_wide.as_ptr(),
            std::ptr::null(),
            &mut reg_type,
            &mut val as *mut u32 as *mut u8,
            &mut buf_len,
        )
    };
    if status != 0 || reg_type != REG_DWORD {
        return None;
    }
    Some(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_by_name_case_insensitive() {
        let apps = vec![
            InstalledApp {
                name: "Google Chrome".into(),
                version: Some("120.0".into()),
                publisher: None,
                install_path: None,
                install_date: None,
                estimated_size_mb: None,
                uninstall_string: None,
                is_per_user: false,
            },
            InstalledApp {
                name: "Visual Studio Code".into(),
                version: None,
                publisher: None,
                install_path: None,
                install_date: None,
                estimated_size_mb: None,
                uninstall_string: None,
                is_per_user: true,
            },
        ];
        let results = find_by_name(&apps, "chrome");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Google Chrome");

        let results = find_by_name(&apps, "VISUAL");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn list_installed_apps_returns_empty_on_non_windows() {
        // On Linux this returns empty — just verify it doesn't crash
        let apps = list_installed_apps();
        #[cfg(not(windows))]
        assert!(apps.is_empty());
        #[cfg(windows)]
        assert!(!apps.is_empty(), "Windows should have installed apps");
    }
}
