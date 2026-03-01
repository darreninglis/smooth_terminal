use anyhow::Result;
use crossbeam_channel::{bounded, Receiver, Sender};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Determine the user's login shell.
///
/// Priority:
///   1. `$SHELL` env var  (set when launched from a terminal)
///   2. `dscl` Directory Services lookup  (works when launched from Finder/Dock)
///   3. Hard-coded `/bin/zsh` fallback
fn get_user_shell() -> String {
    // 1. Env var — fastest, always correct when launched from a terminal
    if let Ok(shell) = std::env::var("SHELL") {
        if !shell.is_empty() {
            return shell;
        }
    }

    // 2. dscl lookup — reliable on macOS even without $SHELL (Finder / Dock launches)
    if let Ok(user) = std::env::var("USER") {
        if let Ok(output) = std::process::Command::new("dscl")
            .args([".", "-read", &format!("/Users/{}", user), "UserShell"])
            .output()
        {
            if let Ok(text) = String::from_utf8(output.stdout) {
                // Output looks like: "UserShell: /bin/zsh"
                if let Some(line) = text.lines().find(|l| l.starts_with("UserShell:")) {
                    if let Some(shell) = line.split_whitespace().nth(1) {
                        return shell.to_string();
                    }
                }
            }
        }
    }

    // 3. Sensible macOS default (the system default since Catalina)
    "/bin/zsh".to_string()
}

pub struct PtyHandle {
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Box<dyn Write + Send>,
    pub child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    pub receiver: Receiver<Vec<u8>>,
    sender: Sender<Vec<u8>>,
}

impl PtyHandle {
    pub fn spawn(cols: u16, rows: u16, cwd: Option<&PathBuf>) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = get_user_shell();
        let mut cmd = CommandBuilder::new(&shell);

        // Spawn as a login shell so that .zprofile / .bash_profile are sourced.
        // This ensures Homebrew PATH, nvm, rbenv, etc. are all available even
        // when the app is launched from Finder or the Dock.
        cmd.arg("-l");

        // Core terminal capabilities
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "smooth_terminal");

        // Ensure critical identity variables are present even in a minimal
        // Finder-launch environment (launchd usually sets HOME/USER, but guard anyway).
        if let Ok(home) = std::env::var("HOME") {
            cmd.env("HOME", home);
        }
        if let Ok(user) = std::env::var("USER") {
            cmd.env("USER", &user);
            cmd.env("LOGNAME", user);
        }
        // Tell the shell what it is (some prompts/tools read $SHELL directly)
        cmd.env("SHELL", &shell);

        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }

        let child = pair.slave.spawn_command(cmd)?;
        let child = Arc::new(Mutex::new(child));

        let master = pair.master;
        let writer = master.take_writer()?;

        let (sender, receiver) = bounded::<Vec<u8>>(256);
        let mut reader = master.try_clone_reader()?;
        let sender_clone = sender.clone();

        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if sender_clone.send(data).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            master,
            writer,
            child,
            receiver,
            sender,
        })
    }

    pub fn write_bytes(&mut self, data: &[u8]) -> Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn try_recv_all(&self) -> Vec<Vec<u8>> {
        let mut chunks = Vec::new();
        while let Ok(chunk) = self.receiver.try_recv() {
            chunks.push(chunk);
        }
        chunks
    }

    /// Get the current working directory of the shell process.
    /// Uses the macOS `proc_pidinfo` API (libproc) for reliability.
    pub fn get_cwd(&self) -> Option<PathBuf> {
        let pid = self.child.lock().ok()?.process_id()? as i32;

        // Use libproc's proc_pidinfo with PROC_PIDVNODEPATHINFO to get cwd
        #[repr(C)]
        struct VnodeInfoPath {
            _vip_vi: [u8; 152],  // struct vnode_info (padding)
            vip_path: [u8; 1024], // MAXPATHLEN
        }
        #[repr(C)]
        struct ProcVnodePathInfo {
            pvi_cdir: VnodeInfoPath,
            pvi_rdir: VnodeInfoPath,
        }
        const PROC_PIDVNODEPATHINFO: i32 = 9;
        extern "C" {
            fn proc_pidinfo(
                pid: i32,
                flavor: i32,
                arg: u64,
                buffer: *mut std::ffi::c_void,
                buffersize: i32,
            ) -> i32;
        }

        let mut info: ProcVnodePathInfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<ProcVnodePathInfo>() as i32;
        let ret = unsafe {
            proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, 0, &mut info as *mut _ as *mut _, size)
        };
        if ret <= 0 {
            return None;
        }

        let bytes = &info.pvi_cdir.vip_path;
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let path = std::str::from_utf8(&bytes[..len]).ok()?;
        if path.is_empty() || path == "/" {
            return None;
        }
        Some(PathBuf::from(path))
    }
}
