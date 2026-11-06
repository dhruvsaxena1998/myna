//! Hugging Face downloads: stream each file into a temp folder, check its size and sha256,
//! then move the whole folder to `models/<id>/` so a model is never half installed.

use crate::models::{self, Model};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub const HF: &str = "https://huggingface.co";

/// Installs `m`, fetching ONNX Runtime first if it needs one that isn't there yet.
/// `fetch` downloads one model (or the runtime) and gets its bytes so far; `progress` gets the percent of the whole.
/// Ok(true) = installed, Ok(false) = canceled.
pub fn install(
    m: &Model,
    models_dir: &Path,
    fetch: &dyn Fn(&Model, &mut dyn FnMut(u64)) -> Result<bool, String>,
    mut progress: impl FnMut(u64),
) -> Result<bool, String> {
    let rt = models::runtime();
    let steps: Vec<&Model> = [(&rt, m.runtime), (m, true)].into_iter().filter(|(x, want)| *want && !models::is_installed(x, models_dir)).map(|(x, _)| x).collect();
    let total = steps.iter().map(|x| x.size).sum::<u64>().max(1);
    let mut before = 0;
    for x in steps {
        if !fetch(x, &mut |n| progress((before + n) * 100 / total))? {
            return Ok(false);
        }
        before += x.size;
    }
    Ok(true)
}

/// Deletes `m`, and ONNX Runtime too once no other model that needs it is installed or downloading.
pub fn remove(m: &Model, models_dir: &Path, downloading: &[String]) -> Result<(), String> {
    let rm = |x: &Model| match std::fs::remove_dir_all(models_dir.join(&x.id)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("Couldn't remove {}: {e}", x.name)),
        _ => Ok(()),
    };
    rm(m)?;
    let needed = models::all().iter().any(|o| o.runtime && (downloading.contains(&o.id) || models::is_installed(o, models_dir)));
    if m.runtime && !needed {
        rm(&models::runtime())?;
    }
    Ok(())
}

/// Ok(true) = installed, Ok(false) = canceled. Either way nothing is left behind unless it's installed.
/// `progress` gets the bytes downloaded so far across all files.
// shortcut: no resume and no read timeout, so a stalled connection only ends on Cancel; add Range resume if models get flaky to fetch.
pub fn fetch(base: &str, m: &Model, models_dir: &Path, cancel: &AtomicBool, mut progress: impl FnMut(u64)) -> Result<bool, String> {
    // Unique per attempt: a canceled attempt may still be winding down when the next one starts.
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let prefix = format!(".downloading-{}-", m.id);
    if let Ok(rd) = std::fs::read_dir(models_dir) {
        for e in rd.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&prefix)) {
            let _ = std::fs::remove_dir_all(e.path()); // left by a crash or an older attempt
        }
    }
    let tmp = models_dir.join(format!("{prefix}{nonce}"));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("Couldn't make room for {}: {e}", m.name))?;
    let done = files(base, m, &tmp, cancel, &mut progress).and_then(|finished| {
        if !finished || cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let dest = models_dir.join(&m.id);
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&tmp, &dest).map_err(|e| format!("Couldn't install {}: {e}", m.name))?;
        Ok(true)
    });
    if !matches!(done, Ok(true)) {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(false); // a newer attempt may have swept our folder, so any error here is just the cancel
    }
    done
}

fn files(base: &str, m: &Model, tmp: &Path, cancel: &AtomicBool, progress: &mut impl FnMut(u64)) -> Result<bool, String> {
    let mut total = 0u64;
    let mut buf = vec![0u8; 256 * 1024];
    for f in &m.files {
        let (url, want_size, want_sha) = match &f.archive {
            Some(a) => (a.url.clone(), a.size, &a.sha256),
            None => (format!("{base}/{}/resolve/{}/{}", m.repo, m.revision, f.path), f.size, &f.sha256),
        };
        let resp = ureq::get(url.as_str()).call().map_err(|e| format!("Couldn't download {}: {e}", m.name))?;
        let mut body = resp.into_body().into_reader();
        let dest = tmp.join(if f.archive.is_some() { ".archive.tgz" } else { &f.path }); // catalog paths may sit in a repo subfolder
        let mut out = std::fs::create_dir_all(dest.parent().unwrap_or(tmp))
            .and_then(|_| std::fs::File::create(&dest))
            .map_err(|e| format!("Couldn't save {}: {e}", m.name))?;
        let (mut hash, mut size) = (Sha256::new(), 0u64);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let n = body.read(&mut buf).map_err(|e| format!("The download of {} was interrupted: {e}", m.name))?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(|e| format!("Couldn't save {}: {e}", m.name))?;
            size += n as u64;
            total += n as u64;
            progress(total);
        }
        if size != want_size || format!("{:x}", hash.finalize()) != *want_sha {
            return Err(format!("{} didn't download correctly (the checksum didn't match). Try again.", m.name));
        }
        if let Some(a) = &f.archive {
            unpack(&dest, &a.member, &tmp.join(&f.path)).map_err(|e| format!("Couldn't unpack {}: {e}", m.name))?;
            let kept = std::fs::read(tmp.join(&f.path)).map_err(|e| format!("Couldn't unpack {}: {e}", m.name))?;
            if kept.len() as u64 != f.size || format!("{:x}", Sha256::digest(&kept)) != f.sha256 {
                return Err(format!("{} didn't unpack correctly. Try again.", m.name));
            }
        }
    }
    Ok(true)
}

/// Takes one file out of a .tgz with macOS's own tar, and leaves the archive and its folders out of `dest`'s folder.
fn unpack(archive: &Path, member: &str, dest: &Path) -> Result<(), String> {
    let dir = dest.parent().ok_or("no folder")?;
    let out = std::process::Command::new("/usr/bin/tar").arg("-xzf").arg(archive).arg("-C").arg(dir).arg(member).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let first = member.split('/').next().unwrap_or(member);
    std::fs::rename(dir.join(member), dest).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(dir.join(first));
    std::fs::remove_file(archive).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::File;
    use std::io::Write;
    use std::net::TcpListener;

    /// Serves `body` to every request (headers ignored) and returns its base URL.
    fn serve(body: Vec<u8>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
                let mut req = [0u8; 2048];
                let _ = s.read(&mut req);
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(&body);
            }
        });
        base
    }

    fn model(body: &[u8], sha256: &str) -> Model {
        Model {
            id: "m".into(), name: "Test model".into(), vendor: String::new(), desc: String::new(), acc: 1, spd: 1, default: false,
            size: body.len() as u64, min_macos: 0, repo: "o/r".into(), revision: "x".repeat(40), system: false, sdk: false, runtime: false, installed: false, needs: None,
            files: vec![File { path: "w.bin".into(), size: body.len() as u64, sha256: sha256.into(), archive: None }],
        }
    }

    // sha256("hello")
    const HELLO: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn entries(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        v.sort();
        v
    }

    #[test]
    fn verified_download_lands_in_the_models_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let (base, mut last) = (serve(b"hello".to_vec()), 0);
        let mut m = model(b"hello", HELLO);
        m.files[0].path = "sub/w.bin".into(); // repo subfolder, like Sortformer's
        assert_eq!(fetch(&base, &m, tmp.path(), &AtomicBool::new(false), |n| last = n), Ok(true));
        assert_eq!(last, 5);
        assert_eq!(std::fs::read(tmp.path().join("m/sub/w.bin")).unwrap(), b"hello");
        assert_eq!(entries(tmp.path()), ["m"]);
    }

    #[test]
    fn checksum_mismatch_fails_and_leaves_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let base = serve(b"hellp".to_vec());
        let err = fetch(&base, &model(b"hellp", HELLO), tmp.path(), &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        assert!(entries(tmp.path()).is_empty());
    }

    #[test]
    fn cancel_leaves_nothing_and_a_failed_request_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let base = serve(b"hello".to_vec());
        assert_eq!(fetch(&base, &model(b"hello", HELLO), tmp.path(), &AtomicBool::new(true), |_| {}), Ok(false));
        assert!(entries(tmp.path()).is_empty());
        let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap(); // closed again at once
        let err = fetch(&format!("http://{dead}"), &model(b"hello", HELLO), tmp.path(), &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(err.contains("Couldn't download"), "{err}");
        assert!(entries(tmp.path()).is_empty());
    }

    #[test]
    fn onnx_models_share_one_runtime_that_goes_with_the_last_of_them() {
        let tmp = tempfile::tempdir().unwrap();
        let log = std::cell::RefCell::new(Vec::<String>::new());
        // A fake download: lays each catalog file down at its catalog size.
        let get = |m: &Model, progress: &mut dyn FnMut(u64)| {
            log.borrow_mut().push(m.id.clone());
            for f in &m.files {
                let path = tmp.path().join(&m.id).join(&f.path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::File::create(path).unwrap().set_len(f.size).unwrap();
            }
            progress(m.size);
            Ok(true)
        };
        let (mut last, runtime) = (0, tmp.path().join("onnxruntime/libonnxruntime.dylib"));
        let go = |id: &str, last: &mut u64| install(&crate::models::find(id).unwrap(), tmp.path(), &get, |p| *last = p);

        assert_eq!(go("whisper-large-v3-turbo", &mut last), Ok(true));
        assert!(!runtime.exists() && *log.borrow() == ["whisper-large-v3-turbo"], "Whisper never downloads it");
        assert_eq!(go("parakeet-tdt-v3", &mut last), Ok(true));
        assert_eq!((last, runtime.exists()), (100, true));
        assert_eq!(log.borrow()[1..], ["onnxruntime", "parakeet-tdt-v3"], "the first ONNX model fetches it, first");
        assert_eq!(go("sortformer", &mut last), Ok(true));
        assert_eq!(log.borrow()[3..], ["sortformer"], "the second doesn't fetch it again");

        let drop = |id: &str, busy: &[String]| remove(&crate::models::find(id).unwrap(), tmp.path(), busy).unwrap();
        drop("parakeet-tdt-v3", &[]);
        assert!(runtime.exists(), "Sortformer still needs it");
        drop("sortformer", &["parakeet-tdt-v3".into()]);
        assert!(runtime.exists(), "Parakeet is downloading again and needs it");
        drop("sortformer", &[]);
        assert!(!runtime.exists() && tmp.path().join("whisper-large-v3-turbo").exists());
    }
}
