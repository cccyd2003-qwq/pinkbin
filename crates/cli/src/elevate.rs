//! UAC 提权重启支持(Windows-only,其他平台为 no-op)。
//!
//! 工作流:
//! 1. agent 调用 `pinkbin-cli --elevate <subcommand> ...`
//! 2. main() 早期调 `elevate::is_elevated()` 检测当前权限
//! 3. 若已提权 → 移除 `--elevate`,正常执行
//! 4. 若未提权 → 创建临时输出文件,`elevate::relaunch_elevated()` 启动
//!    ShellExecuteExW "runas",子进程通过 `--elevated-output <path>` 把
//!    stdout 重定向到该文件;父进程 WaitForSingleObject 等待,读取文件
//!    内容写到自己的 stdout,透传退出码
//!
//! 注意:Rust 的 stdout 在第一次使用时缓存 STD_OUTPUT_HANDLE,所以子进程
//! 必须在任何 println! 之前调 `elevate::redirect_stdout_to_file()`,否则
//! 缓存的旧 handle 不会被替换。

use std::path::Path;

// windows-sys 0.59 中部分常量散落在不同模块/feature 里,为避免 features
// 拼图,这里直接用数值定义(windows SDK 一致的稳定值)。
const SW_SHOWNORMAL: i32 = 1; // <winuser.h> SW_SHOWNORMAL
const GENERIC_WRITE: u32 = 0x4000_0000; // <winnt.h> GENERIC_WRITE
const INFINITE: u32 = 0xFFFF_FFFF; // <winbase.h> INFINITE

/// 当前进程是否以管理员令牌运行。
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    // 非 Windows 上没有 UAC 概念,默认"已提权"避免无限循环。
    true
}

/// 以管理员权限重启自己,等待子进程退出并返回其退出码。
///
/// `args_for_child`:传给子进程的参数(已移除 `--elevate`,已追加
/// `--elevated-output <path>`)。第一个元素通常是子命令名(scan/...)。
#[cfg(windows)]
pub fn relaunch_elevated(args_for_child: &[String]) -> std::io::Result<i32> {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };

    let exe = std::env::current_exe()?;
    let exe_wide: Vec<u16> = exe
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // 拼接参数串(Windows 命令行风格:含空格/制表符的参数用双引号包裹)。
    let mut params = String::new();
    for a in args_for_child {
        if a.is_empty() {
            continue;
        }
        if a.contains(' ') || a.contains('\t') {
            params.push_str(&format!("\"{}\" ", a));
        } else {
            params.push_str(&format!("{} ", a));
        }
    }
    let params = params.trim_end().to_string();
    let params_wide: Vec<u16> = if params.is_empty() {
        Vec::new()
    } else {
        params.encode_utf16().chain(std::iter::once(0)).collect()
    };
    let verb_wide: Vec<u16> = "runas".encode_utf16().chain(std::iter::once(0)).collect();

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb_wide.as_ptr();
    info.lpFile = exe_wide.as_ptr();
    info.lpParameters = if params.is_empty() {
        std::ptr::null()
    } else {
        params_wide.as_ptr()
    };
    info.nShow = SW_SHOWNORMAL;

    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        return Err(std::io::Error::new(
            err.kind(),
            format!(
                "ShellExecuteExW 'runas' 失败: {} (用户可能拒绝了 UAC 提示)",
                err
            ),
        ));
    }

    let h_process = info.hProcess;
    if h_process.is_null() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "ShellExecuteExW 成功但 hProcess 为 null(无法等待子进程)",
        ));
    }

    let wait_result = unsafe { WaitForSingleObject(h_process, INFINITE) };
    let exit_code = if wait_result == WAIT_OBJECT_0 {
        let mut code: u32 = 0;
        let got = unsafe { GetExitCodeProcess(h_process, &mut code) };
        let err = if got == 0 {
            Some(std::io::Error::last_os_error())
        } else {
            None
        };
        unsafe {
            CloseHandle(h_process);
        }
        if let Some(e) = err {
            return Err(e);
        }
        code as i32
    } else {
        let err = std::io::Error::last_os_error();
        unsafe {
            CloseHandle(h_process);
        }
        return Err(err);
    };

    Ok(exit_code)
}

#[cfg(not(windows))]
pub fn relaunch_elevated(_args_for_child: &[String]) -> std::io::Result<i32> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "提权(--elevate)只在 Windows 平台支持",
    ))
}

/// 把当前进程的 stdout 重定向到指定文件。
///
/// 必须在任何 println! / serde_json::to_writer(stdout) 调用之前调用,
/// 因为 Rust 的 stdout 在首次使用时会缓存 STD_OUTPUT_HANDLE,之后
/// SetStdHandle 调用对已缓存的 stdout 无效。
#[cfg(windows)]
pub fn redirect_stdout_to_file(path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, CreateFileW,
    };
    use windows_sys::Win32::System::Console::{STD_OUTPUT_HANDLE, SetStdHandle};

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);

    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { SetStdHandle(STD_OUTPUT_HANDLE, handle) } == 0 {
        let err = std::io::Error::last_os_error();
        unsafe {
            CloseHandle(handle);
        }
        return Err(err);
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn redirect_stdout_to_file(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
