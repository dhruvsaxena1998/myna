//! Curated model downloads: fetch from Hugging Face, verify SHA-256, install atomically.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{self, Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

pub struct File<'a> {
    pub name: &'a str,
    pub url: &'a str,
    pub sha256: &'a str,
    pub size: u64,
}

pub struct Model {
    pub id: &'static str,
    pub task: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub files: &'static [File<'static>],
}

// URLs are pinned to a repo commit so the checksums stay valid; hashes are the LFS oids
// from the Hugging Face API (vocab.txt is not LFS, so its hash was computed from the download).
pub static MODELS: &[Model] = &[
    Model {
        id: "parakeet-tdt-0.6b-v3",
        task: "stt",
        name: "Parakeet TDT 0.6B v3",
        desc: "Default. 25 languages. Fast and accurate for dictation and meetings.",
        files: &[
            File {
                name: "encoder-model.int8.onnx",
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.int8.onnx",
                sha256: "6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09",
                size: 652_183_999,
            },
            File {
                name: "decoder_joint-model.int8.onnx",
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.int8.onnx",
                sha256: "eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70",
                size: 18_202_004,
            },
            File {
                name: "vocab.txt",
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt",
                sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
                size: 93_939,
            },
        ],
    },
    Model {
        id: "whisper-large-v3-turbo",
        task: "stt",
        name: "Whisper large-v3-turbo",
        desc: "99 languages. Better for non-European speech. Slower.",
        files: &[File {
            name: "ggml-large-v3-turbo.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo.bin",
            sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
            size: 1_624_555_275,
        }],
    },
    Model {
        id: "sortformer-4spk-v2.1",
        task: "diarization",
        name: "Sortformer 4-speaker",
        desc: "Open fallback from NVIDIA. Up to four speakers.",
        files: &[File {
            name: "diar_streaming_sortformer_4spk-v2.1.onnx",
            url: "https://huggingface.co/altunenes/parakeet-rs/resolve/b53b9101fa87271041773b5036aeb8db9d810d40/diar_streaming_sortformer_4spk-v2.1.onnx",
            sha256: "f010a62654fd6d2adb70166265df2e53f5a766e354d2cce152b4c7966a04d02d",
            size: 492_243_003,
        }],
    },
];

#[derive(Default)]
pub struct Job {
    pub cancel: AtomicBool,
    pub done: AtomicU64,
    /// None while running; Some(Err(msg)) is kept so the UI can show why a download failed.
    pub result: Mutex<Option<Result<(), String>>>,
}

pub type Jobs = Mutex<HashMap<String, Arc<Job>>>;

#[derive(Debug, PartialEq)]
pub enum Stop {
    Cancelled,
    Failed(String),
}

impl From<io::Error> for Stop {
    fn from(e: io::Error) -> Self {
        Stop::Failed(e.to_string())
    }
}

fn download_file(dest: &Path, f: &File, mut src: impl Read, job: &Job) -> Result<(), Stop> {
    let mut out = fs::File::create(dest)?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if job.cancel.load(Ordering::Relaxed) {
            return Err(Stop::Cancelled);
        }
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        out.write_all(&buf[..n])?;
        job.done.fetch_add(n as u64, Ordering::Relaxed);
    }
    let got = hex(&hash.finalize());
    if got != f.sha256 {
        return Err(Stop::Failed(format!("Checksum mismatch for {}; the download was discarded.", f.name)));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Download every file into `<id>.partial`, then rename to `<id>`. Any failure or cancel
/// deletes the partial directory, so nothing half-installed is ever left behind.
pub fn install<R: Read>(
    root: &Path,
    id: &str,
    files: &[File],
    open: impl Fn(&str) -> io::Result<R>,
    job: &Job,
) -> Result<(), Stop> {
    let part = root.join(format!("{id}.partial"));
    let dest = root.join(id);
    let _ = fs::remove_dir_all(&part);
    fs::create_dir_all(&part)?;
    let res = (|| {
        for f in files {
            download_file(&part.join(f.name), f, open(f.url)?, job)?;
        }
        let _ = fs::remove_dir_all(&dest); // replaces a damaged copy only now that the new one verified
        Ok(fs::rename(&part, &dest)?)
    })();
    if res.is_err() {
        let _ = fs::remove_dir_all(&part);
    }
    res
}

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Absent,
    Installed,
    Damaged,
}

/// Cheap check (files present, right size). `verify` does the full hash.
pub fn status(root: &Path, m: &Model) -> Status {
    let dir = root.join(m.id);
    if !dir.exists() {
        return Status::Absent;
    }
    let ok = m
        .files
        .iter()
        .all(|f| fs::metadata(dir.join(f.name)).map(|md| md.len() == f.size).unwrap_or(false));
    if ok { Status::Installed } else { Status::Damaged }
}

pub fn verify(root: &Path, m: &Model) -> bool {
    m.files.iter().all(|f| {
        let Ok(mut file) = fs::File::open(root.join(m.id).join(f.name)) else { return false };
        let mut hash = Sha256::new();
        io::copy(&mut file, &mut hash).is_ok() && hex(&hash.finalize()) == f.sha256
    })
}

pub fn remove(root: &Path, id: &str) {
    let _ = fs::remove_dir_all(root.join(id));
}

/// Leftovers from a crash or force-quit mid-download.
pub fn clean_partials(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for e in entries.flatten() {
        if e.file_name().to_string_lossy().ends_with(".partial") {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}

pub fn http_open(url: &str) -> io::Result<Box<dyn Read + Send + Sync>> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(30))
        .build();
    let resp = agent.get(url).call().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    Ok(resp.into_reader())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("myna-models-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    const HELLO_SHA: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    const HELLO: File = File { name: "a.bin", url: "mem://a", sha256: HELLO_SHA, size: 5 };

    #[test]
    fn good_download_installs_and_verifies() {
        let root = tmp("ok");
        let job = Job::default();
        install(&root, "m", &[HELLO], |_| Ok(Cursor::new(b"hello".to_vec())), &job).unwrap();
        assert_eq!(fs::read(root.join("m/a.bin")).unwrap(), b"hello");
        assert!(!root.join("m.partial").exists());
        assert_eq!(job.done.load(Ordering::Relaxed), 5);
        let m = Model { id: "m", task: "stt", name: "", desc: "", files: &[HELLO] };
        assert_eq!(status(&root, &m), Status::Installed);
        assert!(verify(&root, &m));
        // Same size, different bytes: cheap check passes, full hash catches it.
        fs::write(root.join("m/a.bin"), b"hellx").unwrap();
        assert_eq!(status(&root, &m), Status::Installed);
        assert!(!verify(&root, &m));
        fs::write(root.join("m/a.bin"), b"hell").unwrap();
        assert_eq!(status(&root, &m), Status::Damaged);
        remove(&root, "m");
        assert_eq!(status(&root, &m), Status::Absent);
    }

    #[test]
    fn checksum_mismatch_fails_and_leaves_nothing() {
        let root = tmp("bad");
        let job = Job::default();
        let err = install(&root, "m", &[HELLO], |_| Ok(Cursor::new(b"HELLO".to_vec())), &job).unwrap_err();
        assert!(matches!(err, Stop::Failed(m) if m.contains("Checksum mismatch")));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    }

    #[test]
    fn cancel_leaves_nothing_and_keeps_existing_install() {
        let root = tmp("cancel");
        fs::create_dir_all(root.join("m")).unwrap();
        fs::write(root.join("m/old"), b"x").unwrap();
        let job = Job::default();
        job.cancel.store(true, Ordering::Relaxed);
        let err = install(&root, "m", &[HELLO], |_| Ok(Cursor::new(b"hello".to_vec())), &job).unwrap_err();
        assert_eq!(err, Stop::Cancelled);
        assert!(!root.join("m.partial").exists());
        assert!(root.join("m/old").exists());
    }

    #[test]
    fn second_file_failure_discards_first() {
        let root = tmp("two");
        let bad = File { name: "b.bin", url: "mem://b", sha256: "00", size: 1 };
        let job = Job::default();
        let r = install(&root, "m", &[HELLO, bad], |_| Ok(Cursor::new(b"hello".to_vec())), &job);
        assert!(r.is_err());
        assert!(!root.join("m").exists() && !root.join("m.partial").exists());
    }

    #[test]
    fn stale_partials_are_cleaned() {
        let root = tmp("stale");
        fs::create_dir_all(root.join("m.partial")).unwrap();
        fs::create_dir_all(root.join("keep")).unwrap();
        clean_partials(&root);
        assert!(!root.join("m.partial").exists() && root.join("keep").exists());
    }

    #[test]
    fn open_error_cleans_up() {
        let root = tmp("open");
        let r = install::<Cursor<Vec<u8>>>(&root, "m", &[HELLO], |_| Err(io::Error::other("offline")), &Job::default());
        assert!(matches!(r, Err(Stop::Failed(_))));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    }

    // Real network: `cargo test -- --ignored`. Checks the pinned vocab.txt URL, redirect handling and hash.
    #[test]
    #[ignore]
    fn real_hugging_face_download() {
        let root = tmp("real");
        let m = &MODELS[0];
        let vocab = &m.files[2];
        install(&root, "v", std::slice::from_ref(vocab), http_open, &Job::default()).unwrap();
        assert!(root.join("v/vocab.txt").exists());
    }
}
