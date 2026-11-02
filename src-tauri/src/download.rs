//! Hugging Face downloads: stream each file into a temp folder, check its size and sha256,
//! then move the whole folder to `models/<id>/` so a model is never half installed.

use crate::models::Model;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub const HF: &str = "https://huggingface.co";

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
        let url = format!("{base}/{}/resolve/{}/{}", m.repo, m.revision, f.path);
        let resp = ureq::get(&url).call().map_err(|e| format!("Couldn't download {}: {e}", m.name))?;
        let mut body = resp.into_body().into_reader();
        let mut out = std::fs::File::create(tmp.join(&f.path)).map_err(|e| format!("Couldn't save {}: {e}", m.name))?;
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
        if size != f.size || format!("{:x}", hash.finalize()) != f.sha256 {
            return Err(format!("{} didn't download correctly (the checksum didn't match). Try again.", m.name));
        }
    }
    Ok(true)
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
            size: body.len() as u64, min_macos: 0, repo: "o/r".into(), revision: "x".repeat(40), installed: false, runs_here: true,
            files: vec![File { path: "w.bin".into(), size: body.len() as u64, sha256: sha256.into() }],
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
        let m = model(b"hello", HELLO);
        assert_eq!(fetch(&base, &m, tmp.path(), &AtomicBool::new(false), |n| last = n), Ok(true));
        assert_eq!(last, 5);
        assert_eq!(std::fs::read(tmp.path().join("m/w.bin")).unwrap(), b"hello");
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
}
