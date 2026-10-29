//! Curated model downloads: fetch from Hugging Face, verify SHA-256, install atomically.
use serde::{Deserialize, Serialize};
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

/// Borrowed so a user's own model (strings built at runtime) can use the same install/status code.
pub struct Model<'a> {
    pub id: &'a str,
    pub task: &'static str,
    pub name: &'a str,
    pub desc: &'a str,
    pub files: &'a [File<'a>],
}

/// An owned `File`, for custom models whose file list is stored in settings.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Remote {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

impl Remote {
    pub fn file(&self) -> File<'_> {
        File { name: &self.name, url: &self.url, sha256: &self.sha256, size: self.size }
    }
}

/// A file of Desert Ant's Voz, at the repo commit behind tag v0.5.0: the revision SDK 4.2.1 pins.
macro_rules! voz_file {
    ($name:literal, $sha256:literal, $size:literal) => {
        File {
            name: $name,
            url: concat!("https://huggingface.co/desert-ant-labs/voz/resolve/12802bf4e2b5e97e1289b723bc6283b6f1a1ace5/", $name),
            sha256: $sha256,
            size: $size,
        }
    };
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
    // The Core ML files only; Myna downloads them and the Desert Ant SDK loads them (see voz.rs).
    // Same non-LFS caveat as above for the small json/mil files.
    Model {
        id: "voz",
        task: "stt",
        name: "Voz",
        desc: "Runs on the Neural Engine. Fastest on Apple Silicon. Needs macOS 15.",
        files: &[
            voz_file!("decoder.mlmodelc/analytics/coremldata.bin", "cc2be0dbcd0c7d8d915aa3621869ea074791c432ba7b4bb6e029c483087f7189", 243),
            voz_file!("decoder.mlmodelc/coremldata.bin", "2d7a5210e91e4e3b25c547db4b76ea14bf769db9f71e7659c027008041e38d2d", 1_093),
            voz_file!("decoder.mlmodelc/metadata.json", "798639c54e4eb71bcfa5dbc15b62c0a341c37b265d1a9c879c28a40b06549875", 18_528),
            voz_file!("decoder.mlmodelc/model.mil", "1f331acb4a4bb999c792db0a3725d790c046311ab9941ddbe0ee7e3418ae1e14", 99_410),
            voz_file!("decoder.mlmodelc/weights/weight.bin", "12ab4cbb83155dbaf3d9832f524c23b803e3e266863d9c6f3d58e1d88577e130", 12_277_132),
            voz_file!("embedding.f16", "07163e5f6f8a68f9ffeed7a8ff770c68098c2db216232321912188bd614b9a72", 10_487_040),
            voz_file!("encoder.mlmodelc/analytics/coremldata.bin", "983e1e4e31f6ef386fba8d35202984c6ec11ff688f62cf3fde57dd957d40ff27", 243),
            voz_file!("encoder.mlmodelc/coremldata.bin", "02327c1dc4dc362dfcb1061f785a24ee3bcb86853ecaa818511ae71ca2e2a0b8", 430),
            voz_file!("encoder.mlmodelc/metadata.json", "81e00f4d507edf2e6d045a9826362f3c6e314e60ef16a28aef4c99d693876991", 2_739),
            voz_file!("encoder.mlmodelc/model.mil", "974c88c39c840c21b031f10529943516d1d8469535f0ddc3571cddaab29574e4", 820_490),
            voz_file!("encoder.mlmodelc/weights/weight.bin", "72b6e867cb3176cea36fc1410622b7bfd50043e80eafbaab03ae0a53382908c2", 312_039_808),
            voz_file!("mel.mlmodelc/analytics/coremldata.bin", "d1cabbea932a1be1b53a1adc0d434d5a7a1f097d9679d3d995183238773de472", 243),
            voz_file!("mel.mlmodelc/coremldata.bin", "e6598e764b01f8ba06fa787f2bc22d7707f97c2954492d77a2736800fd006f5d", 405),
            voz_file!("mel.mlmodelc/metadata.json", "6be5be2c02370c3a218b712046ea50c8486f4e20fd70558458d5980d1c296e53", 2_215),
            voz_file!("mel.mlmodelc/model.mil", "aae23c58b6aa3093063f75a392d9f44deb324804c3fbb58fa3e55bb7f371ece1", 9_728),
            voz_file!("mel.mlmodelc/weights/weight.bin", "34f8a1bcf01e6a6ae053efe012cd189e0c38e9dca65660f199740ce9a0958f95", 826_688),
            voz_file!("meta.json", "2b5fa951d70cbbab49fccf31836521be8305d0d9557b9bedf45cc8dcdc61b53b", 584),
            voz_file!("vocab.json", "3eb2f73e8853dce5156c2ed97c0b1bb84eb4ebd201eb1761d15977fc1ea31490", 78_654),
        ],
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
    // Names may be nested (Core ML models are directories).
    fs::create_dir_all(dest.parent().unwrap_or(dest))?;
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
    // shortcut: an empty sha256 (small non-LFS files from a custom repo, which Hugging Face only
    // hashes with git's SHA-1) is size-checked only; upgrade to blob SHA-1 if those files matter.
    if !f.sha256.is_empty() && hex(&hash.finalize()) != f.sha256 {
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
        io::copy(&mut file, &mut hash).is_ok() && (f.sha256.is_empty() || hex(&hash.finalize()) == f.sha256)
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
    fn nested_file_names_install_and_verify() {
        let root = tmp("nested");
        let nested = File { name: "a.mlmodelc/weights/w.bin", ..HELLO };
        install(&root, "m", &[nested], |_| Ok(Cursor::new(b"hello".to_vec())), &Job::default()).unwrap();
        assert_eq!(fs::read(root.join("m/a.mlmodelc/weights/w.bin")).unwrap(), b"hello");
        let m = Model { id: "m", task: "stt", name: "", desc: "", files: &[File { name: "a.mlmodelc/weights/w.bin", ..HELLO }] };
        assert_eq!(status(&root, &m), Status::Installed);
        assert!(verify(&root, &m));
    }

    #[test]
    fn file_without_a_checksum_is_size_checked_only() {
        let root = tmp("nosum");
        let f = File { name: "a.bin", url: "mem://a", sha256: "", size: 5 };
        install(&root, "m", &[f], |_| Ok(Cursor::new(b"hello".to_vec())), &Job::default()).unwrap();
        let f = File { name: "a.bin", url: "mem://a", sha256: "", size: 5 };
        let m = Model { id: "m", task: "stt", name: "", desc: "", files: &[f] };
        assert_eq!(status(&root, &m), Status::Installed);
        assert!(verify(&root, &m));
        fs::write(root.join("m/a.bin"), b"hell").unwrap();
        assert_eq!(status(&root, &m), Status::Damaged);
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
