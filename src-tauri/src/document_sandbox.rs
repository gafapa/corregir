//! Windows AppContainer with no network capabilities and a read-only source handle.
use crate::document_worker::{WorkerJob, WorkerRequest};
use std::{
    ffi::c_void,
    fs::File,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    process::ExitStatus,
    ptr,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, Isolation::*, *},
    Storage::FileSystem::{FILE_GENERIC_EXECUTE, FILE_GENERIC_READ},
    System::{Pipes::CreatePipe, Threading::*},
};
static SANDBOX_ACCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
fn failure(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
}
fn owned(handle: HANDLE) -> Result<OwnedHandle, String> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(failure("invalid document worker handle"));
    }
    // SAFETY: a successful Win32 call transferred ownership of this handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}
struct Profile {
    name: Vec<u16>,
    sid: PSID,
}
impl Profile {
    fn create() -> Result<Self, String> {
        use rand::RngExt;
        let name = wide(std::ffi::OsStr::new(&format!(
            "Corregir.Document.{:016x}",
            rand::rng().random::<u64>()
        )));
        let mut sid = ptr::null_mut();
        // SAFETY: terminated strings and no requested capabilities.
        let result = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                name.as_ptr(),
                name.as_ptr(),
                ptr::null(),
                0,
                &mut sid,
            )
        };
        if result < 0 {
            return Err(format!(
                "could not create restricted document profile: {result:#x}"
            ));
        }
        Ok(Self { name, sid })
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        // SAFETY: the worker is stopped; this owns the profile SID allocation.
        unsafe {
            FreeSid(self.sid);
            if DeleteAppContainerProfile(self.name.as_ptr()) < 0 {
                eprintln!("warning: could not remove document sandbox profile");
            }
        }
    }
}
struct ReadGrant {
    path: Vec<u16>,
    descriptor: PSECURITY_DESCRIPTOR,
    sid: PSID,
    applied: bool,
}
impl ReadGrant {
    fn new(path: &Path, sid: PSID) -> Result<Self, String> {
        let path = wide(path.as_os_str());
        let mut descriptor = ptr::null_mut();
        let mut dacl = ptr::null_mut();
        // SAFETY: outputs receive an allocated descriptor and a DACL borrowed from it.
        let result = unsafe {
            GetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut dacl,
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        if result != ERROR_SUCCESS {
            return Err(format!(
                "could not read bundled-resource permissions: {result}"
            ));
        }
        let mut grant = Self {
            path,
            descriptor,
            sid,
            applied: false,
        };
        if dacl.is_null() {
            return Err("cannot restrict a resource with an unrestricted DACL".into());
        }
        let mut entry = EXPLICIT_ACCESS_W::default();
        entry.grfAccessPermissions = FILE_GENERIC_READ | FILE_GENERIC_EXECUTE;
        entry.grfAccessMode = GRANT_ACCESS;
        entry.grfInheritance = NO_INHERITANCE;
        entry.Trustee.TrusteeForm = TRUSTEE_IS_SID;
        entry.Trustee.TrusteeType = TRUSTEE_IS_UNKNOWN;
        entry.Trustee.ptstrName = sid as _;
        let mut updated = ptr::null_mut();
        // SAFETY: the SID lives through launch; SetNamedSecurityInfo copies the ACL.
        let result = unsafe { SetEntriesInAclW(1, &entry, dacl, &mut updated) };
        if result != ERROR_SUCCESS {
            return Err(format!("could not prepare resource-only access: {result}"));
        }
        let result = unsafe {
            SetNamedSecurityInfoW(
                grant.path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                updated,
                ptr::null(),
            )
        };
        unsafe {
            LocalFree(updated as _);
        }
        if result != ERROR_SUCCESS {
            return Err(format!(
                "could not grant read-only bundled resources: {result}"
            ));
        }
        grant.applied = true;
        Ok(grant)
    }
}
impl Drop for ReadGrant {
    fn drop(&mut self) {
        // Revoke only this unique sandbox SID, preserving concurrent permission changes.
        unsafe {
            if self.applied {
                let mut current = ptr::null_mut();
                let mut descriptor = ptr::null_mut();
                let result = GetNamedSecurityInfoW(
                    self.path.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    &mut current,
                    ptr::null_mut(),
                    &mut descriptor,
                );
                if result == ERROR_SUCCESS {
                    let mut entry = EXPLICIT_ACCESS_W::default();
                    entry.grfAccessMode = REVOKE_ACCESS;
                    entry.Trustee.TrusteeForm = TRUSTEE_IS_SID;
                    entry.Trustee.ptstrName = self.sid as _;
                    let mut updated = ptr::null_mut();
                    if SetEntriesInAclW(1, &entry, current, &mut updated) == ERROR_SUCCESS {
                        if SetNamedSecurityInfoW(
                            self.path.as_ptr(),
                            SE_FILE_OBJECT,
                            DACL_SECURITY_INFORMATION,
                            ptr::null_mut(),
                            ptr::null_mut(),
                            updated,
                            ptr::null(),
                        ) != ERROR_SUCCESS
                        {
                            eprintln!(
                                "warning: could not revoke bundled-resource sandbox permissions"
                            );
                        }
                        LocalFree(updated as _);
                    } else {
                        eprintln!("warning: could not prepare bundled-resource permission cleanup");
                    }
                    LocalFree(descriptor);
                } else {
                    eprintln!(
                        "warning: could not read bundled-resource permissions during cleanup"
                    );
                }
            }
            LocalFree(self.descriptor);
        }
    }
}
struct Attributes {
    _storage: Vec<usize>,
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
}
impl Attributes {
    fn new() -> Result<Self, String> {
        let mut bytes = 0;
        // SAFETY: first call obtains size; usize storage supplies alignment.
        unsafe {
            InitializeProcThreadAttributeList(ptr::null_mut(), 2, 0, &mut bytes);
        }
        let mut storage = vec![0usize; bytes.div_ceil(std::mem::size_of::<usize>())];
        let list = storage.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
        if unsafe { InitializeProcThreadAttributeList(list, 2, 0, &mut bytes) } == 0 {
            return Err(failure(
                "could not initialize restricted process attributes",
            ));
        }
        Ok(Self {
            _storage: storage,
            list,
        })
    }
    fn set(&self, kind: u32, value: *const c_void, size: usize) -> Result<(), String> {
        // SAFETY: the caller retains values until CreateProcessW completes.
        if unsafe {
            UpdateProcThreadAttribute(
                self.list,
                0,
                kind as usize,
                value,
                size,
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(failure("could not set restricted process attributes"));
        }
        Ok(())
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.list);
        }
    }
}
fn pipe() -> Result<(OwnedHandle, OwnedHandle), String> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: ptr::null_mut(),
        bInheritHandle: 1,
    };
    let mut read = ptr::null_mut();
    let mut write = ptr::null_mut();
    // SAFETY: output handles transfer to RAII wrappers.
    if unsafe { CreatePipe(&mut read, &mut write, &attributes, 0) } == 0 {
        return Err(failure("could not create restricted document pipe"));
    }
    Ok((owned(read)?, owned(write)?))
}
fn inherit(handle: &OwnedHandle, enabled: bool) -> Result<(), String> {
    if unsafe {
        SetHandleInformation(
            handle.as_raw_handle(),
            HANDLE_FLAG_INHERIT,
            if enabled { HANDLE_FLAG_INHERIT } else { 0 },
        )
    } == 0
    {
        return Err(failure("could not configure document handle inheritance"));
    }
    Ok(())
}
pub struct SandboxedChild {
    pub stdin: Option<File>,
    pub stdout: Option<File>,
    process: OwnedHandle,
    _job: Option<WorkerJob>,
    _grants: Vec<ReadGrant>,
    _profile: Profile,
    _access: std::sync::MutexGuard<'static, ()>,
}
impl SandboxedChild {
    pub fn spawn(
        executable: &Path,
        request: &mut WorkerRequest,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Self, String> {
        let started = std::time::Instant::now();
        let access = loop {
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("import cancelled".into());
            }
            if started.elapsed() > std::time::Duration::from_secs(600) {
                return Err("document sandbox queue timed out".into());
            }
            match SANDBOX_ACCESS.try_lock() {
                Ok(guard) => break guard,
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err("document sandbox access lock failed".into())
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(std::time::Duration::from_millis(50))
                }
            }
        };
        let profile = Profile::create()?;
        let paths = [
            executable.to_path_buf(),
            executable
                .parent()
                .ok_or("missing executable directory")?
                .to_path_buf(),
            request.pdfium_directory.clone(),
            request.pdfium_directory.join("pdfium.dll"),
            request.models_directory.clone(),
            request.models_directory.join("text-detection.rten"),
            request.models_directory.join("text-recognition.rten"),
        ];
        let mut grants = Vec::new();
        for path in paths.into_iter().collect::<std::collections::BTreeSet<_>>() {
            grants.push(ReadGrant::new(&path, profile.sid)?);
        }
        crate::pipeline::render::validate_file(&request.file_path)
            .map_err(|error| error.to_string())?;
        let source: OwnedHandle = File::open(&request.file_path)
            .map_err(|error| error.to_string())?
            .into();
        inherit(&source, true)?;
        let (input_read, input_write) = pipe()?;
        inherit(&input_write, false)?;
        let (output_read, output_write) = pipe()?;
        inherit(&output_read, false)?;
        let capabilities = SECURITY_CAPABILITIES {
            AppContainerSid: profile.sid,
            Capabilities: ptr::null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        };
        let null: OwnedHandle = std::fs::OpenOptions::new()
            .write(true)
            .open("NUL")
            .map_err(|error| error.to_string())?
            .into();
        inherit(&null, true)?;
        let handles = [
            source.as_raw_handle(),
            input_read.as_raw_handle(),
            output_write.as_raw_handle(),
            null.as_raw_handle(),
        ];
        let attributes = Attributes::new()?;
        attributes.set(
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
            &capabilities as *const _ as _,
            std::mem::size_of_val(&capabilities),
        )?;
        attributes.set(
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            handles.as_ptr() as _,
            std::mem::size_of_val(&handles),
        )?;
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = input_read.as_raw_handle();
        startup.StartupInfo.hStdOutput = output_write.as_raw_handle();
        startup.StartupInfo.hStdError = null.as_raw_handle();
        startup.lpAttributeList = attributes.list;
        let executable_wide = wide(executable.as_os_str());
        let mut command: Vec<u16> = std::iter::once('"' as u16)
            .chain(executable.as_os_str().encode_wide())
            .chain("\" --document-worker\0".encode_utf16())
            .collect();
        let directory = wide(executable.parent().unwrap().as_os_str());
        let mut environment = Vec::new();
        // Windows profile expansion requires these standard path variables; no credentials or PATH are inherited.
        for key in [
            "APPDATA",
            "LOCALAPPDATA",
            "SystemDrive",
            "SystemRoot",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "windir",
        ] {
            if let Some(value) = std::env::var_os(key) {
                environment.extend(key.encode_utf16());
                environment.push('=' as u16);
                environment.extend(value.encode_wide());
                environment.push(0);
            }
        }
        environment.push(0);
        let mut info = PROCESS_INFORMATION::default();
        // SAFETY: strings, pipes, capabilities and attribute storage remain alive through creation.
        if unsafe {
            CreateProcessW(
                executable_wide.as_ptr(),
                command.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT
                    | CREATE_UNICODE_ENVIRONMENT
                    | CREATE_NO_WINDOW
                    | CREATE_SUSPENDED,
                environment.as_ptr() as _,
                directory.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            )
        } == 0
        {
            return Err(failure("could not start AppContainer document worker"));
        }
        let process = owned(info.hProcess)?;
        let thread = owned(info.hThread)?;
        let mut child = Self {
            stdin: Some(File::from(input_write)),
            stdout: Some(File::from(output_read)),
            process,
            _job: None,
            _grants: grants,
            _profile: profile,
            _access: access,
        };
        child._job = Some(WorkerJob::attach_handle(child.handle())?);
        // SAFETY: job limits are in force before any document worker instructions execute.
        if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
            return Err(failure("could not resume restricted document worker"));
        }
        request.source_handle = Some(source.as_raw_handle() as usize as u64);
        Ok(child)
    }
    pub fn handle(&self) -> HANDLE {
        self.process.as_raw_handle()
    }
    pub fn kill(&mut self) -> std::io::Result<()> {
        if unsafe { TerminateProcess(self.handle(), 1) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        use std::os::windows::process::ExitStatusExt;
        if unsafe { WaitForSingleObject(self.handle(), 0) } == WAIT_TIMEOUT {
            return Ok(None);
        }
        let mut code = 0;
        if unsafe { GetExitCodeProcess(self.handle(), &mut code) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Some(ExitStatus::from_raw(code)))
    }
    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        unsafe {
            WaitForSingleObject(self.handle(), INFINITE);
        }
        self.try_wait()?
            .ok_or_else(|| std::io::Error::other("document process did not finish"))
    }
}
impl Drop for SandboxedChild {
    fn drop(&mut self) {
        let _ = self.kill();
        let _ = self.wait();
    }
}
