//! Laya, the local decision model, as a managed Python sidecar: an app-owned uv environment, and `laya-serve` run as a
//! child process on a localhost port with a generated key, started when triage first needs it and stopped on quit.
use crate::ai::keychain;
use serde::Serialize;
use std::{
    io::Read,
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

// shortcut: the package name, the `laya-serve` flags and the key variable are taken from the spec, not from Laya's
// docs, and the multilingual checkpoint downloads on first start (hence the long wait). Fix them here.
const PACKAGE: &str = "laya";
const SERVER: &str = "laya-serve";
const KEY_ENV: &str = "LAYA_API_KEY";
const READY_WITHIN: Duration = Duration::from_secs(180);

pub struct Laya {
    dir: PathBuf,
    /// What the wizard is doing, or why it stopped.
    setup: Mutex<Setup>,
    running: Mutex<Option<Running>>,
}

#[derive(Default)]
struct Setup {
    step: Option<String>,
    error: Option<String>,
}

struct Running {
    child: Child,
    port: u16,
    key: String,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Serialize)]
pub struct Status {
    installed: bool,
    /// The wizard's current step; None when it is not running.
    step: Option<String>,
    error: Option<String>,
    running: bool,
    dir: String,
}

/// The arguments `laya-serve` gets: loopback only, whatever else the host does. The key goes in the environment, where
/// `ps` does not show it.
fn serve_args(port: u16) -> Vec<String> {
    ["--host", "127.0.0.1", "--port", &port.to_string(), "--checkpoint", "multilingual"].map(Into::into).to_vec()
}

fn hex_key() -> Result<String, String> {
    let mut bytes = [0u8; 24];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).map_err(|e| format!("No random key: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    Ok(listener.local_addr().map_err(|e| e.to_string())?.port())
}

/// Waits for `port` to accept connections, or for the child to die.
fn wait_ready(child: &mut Child, port: u16, within: Duration) -> Result<(), String> {
    let until = Instant::now() + within;
    while Instant::now() < until {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("Laya stopped with {status} before it was ready."));
        }
        if TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_millis(200)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err("Laya did not become ready in time.".into())
}

/// `uv` lives outside the app's PATH when launched from Finder, so the usual install folders are tried too.
fn find_uv(path: Option<std::ffi::OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = path.iter().flat_map(std::env::split_paths).collect();
    dirs.extend(home.iter().flat_map(|h| [h.join(".local/bin"), h.join(".cargo/bin")]));
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    dirs.into_iter().map(|d| d.join("uv")).find(|p| p.is_file())
}

/// Runs a setup command; a failure is its last line of stderr.
fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.stdin(Stdio::null()).output().map_err(|e| format!("Could not run {:?}: {e}", cmd.get_program()))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("it failed without saying why").trim().to_string())
}

impl Laya {
    pub fn new(dir: PathBuf) -> Laya {
        Laya { dir, setup: Default::default(), running: Default::default() }
    }

    fn venv(&self) -> PathBuf {
        self.dir.join("venv")
    }

    fn server(&self) -> PathBuf {
        self.venv().join("bin").join(SERVER)
    }

    pub fn installed(&self) -> bool {
        self.server().is_file()
    }

    pub fn status(&self) -> Status {
        let setup = self.setup.lock().unwrap();
        Status {
            installed: self.installed(),
            step: setup.step.clone(),
            error: setup.error.clone(),
            running: self.running.lock().unwrap().as_mut().is_some_and(|r| r.child.try_wait().ok().flatten().is_none()),
            dir: self.dir.display().to_string(),
        }
    }

    /// Creates the environment and installs Laya into it. Blocks; the wizard runs it on a thread and polls `status`.
    fn install(&self) -> Result<(), String> {
        let uv = find_uv(std::env::var_os("PATH"), std::env::var_os("HOME").map(PathBuf::from))
            .ok_or("Laya's setup needs uv. Install it (brew install uv), then try again.")?;
        let step = |s: &str| self.setup.lock().unwrap().step = Some(s.into());
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        step("Creating the environment");
        run(Command::new(&uv).arg("venv").arg(self.venv()))?;
        step("Installing Laya");
        run(Command::new(&uv).args(["pip", "install", "--python"]).arg(self.venv().join("bin/python")).arg(PACKAGE))
    }

    /// Where Laya listens and the key it wants, starting it if it is not running. Blocks until it answers.
    pub fn serve(&self, key_account: &str) -> Result<(String, String), String> {
        let mut slot = self.running.lock().unwrap();
        if let Some(r) = slot.as_mut() {
            if r.child.try_wait().ok().flatten().is_none() {
                return Ok((format!("http://127.0.0.1:{}", r.port), r.key.clone()));
            }
        }
        *slot = None;
        let key = match keychain::get(key_account) {
            Some(k) => k,
            None => {
                let k = hex_key()?;
                keychain::set(key_account, &k)?;
                k
            }
        };
        let port = free_port()?;
        let child = Command::new(self.server())
            .args(serve_args(port))
            .env(KEY_ENV, &key)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Could not start Laya: {e}"))?;
        let mut running = Running { child, port, key };
        wait_ready(&mut running.child, port, READY_WITHIN)?; // a failure drops `running`, which kills the child
        let at = (format!("http://127.0.0.1:{port}"), running.key.clone());
        *slot = Some(running);
        Ok(at)
    }

    pub fn stop(&self) {
        *self.running.lock().unwrap() = None;
    }
}

/// Quit path: Laya does not outlive Myna.
pub fn shutdown(app: &AppHandle) {
    app.state::<Laya>().stop();
}

/// Starts the wizard on a thread; a second press while it runs does nothing.
#[tauri::command]
pub fn laya_setup(app: AppHandle) {
    let laya = app.state::<Laya>();
    {
        let mut setup = laya.setup.lock().unwrap();
        if setup.step.is_some() {
            return;
        }
        *setup = Setup { step: Some("Starting".into()), error: None };
    }
    std::thread::spawn(move || {
        let laya = app.state::<Laya>();
        let result = laya.install();
        *laya.setup.lock().unwrap() = Setup { step: None, error: result.err() };
    });
}

#[tauri::command]
pub fn laya_stop(laya: tauri::State<Laya>) {
    laya.stop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn laya_listens_on_loopback_only_and_the_key_stays_out_of_the_arguments() {
        let args = serve_args(8123);
        let at = |flag: &str| args.iter().position(|a| a == flag).map(|i| args[i + 1].as_str());
        assert_eq!((at("--host"), at("--port"), at("--checkpoint")), (Some("127.0.0.1"), Some("8123"), Some("multilingual")));
        assert!(!args.iter().any(|a| a == "0.0.0.0" || a.to_lowercase().contains("key")));
    }

    #[test]
    fn generated_keys_are_long_hex_and_differ() {
        let (a, b) = (hex_key().unwrap(), hex_key().unwrap());
        assert!(a.len() == 48 && a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn a_ready_server_is_noticed_and_a_dead_or_silent_one_is_reported() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut alive = Command::new("sleep").arg("30").spawn().unwrap();
        assert_eq!(wait_ready(&mut alive, port, Duration::from_secs(5)), Ok(()));
        let silent = 1; // privileged, so nothing in a parallel test can be listening there
        assert!(wait_ready(&mut alive, silent, Duration::from_millis(400)).unwrap_err().contains("not become ready"));
        alive.kill().unwrap();
        let _ = alive.wait();
        assert!(wait_ready(&mut alive, silent, Duration::from_secs(5)).unwrap_err().contains("stopped"));
    }

    #[test]
    fn stopping_kills_the_child() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        let laya = Laya::new(std::env::temp_dir());
        *laya.running.lock().unwrap() = Some(Running { child, port: 1, key: "k".into() });
        assert!(laya.status().running);
        laya.stop();
        assert!(!laya.status().running);
        // The pid is gone once the child has been waited on.
        assert_ne!(unsafe { libc::kill(pid as i32, 0) }, 0);
    }

    #[test]
    fn uv_is_found_on_the_path_or_in_the_usual_folders() {
        let root = std::env::temp_dir().join(format!("myna-uv-{}", std::process::id()));
        let bin = root.join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("uv"), "").unwrap();
        assert_eq!(find_uv(None, Some(root.clone())), Some(bin.join("uv")));
        let other = root.join("elsewhere");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("uv"), "").unwrap();
        assert_eq!(find_uv(Some(other.clone().into()), Some(root.clone())), Some(other.join("uv")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_setup_command_reports_its_last_line() {
        assert_eq!(run(Command::new("sh").args(["-c", "echo noise >&2; echo 'no such package' >&2; exit 2"])), Err("no such package".into()));
        assert_eq!(run(&mut Command::new("true")), Ok(()));
    }

    #[test]
    fn it_is_installed_once_the_server_binary_exists() {
        let dir = std::env::temp_dir().join(format!("myna-laya-{}", std::process::id()));
        let laya = Laya::new(dir.clone());
        assert!(!laya.status().installed);
        fs::create_dir_all(laya.server().parent().unwrap()).unwrap();
        fs::write(laya.server(), "").unwrap();
        assert!(laya.status().installed);
        let _ = fs::remove_dir_all(&dir);
    }
}
