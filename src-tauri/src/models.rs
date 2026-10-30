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

/// A file of Desert Ant's Uhm, at the repo commit behind tag v1.2.0.
macro_rules! uhm_file {
    ($name:literal, $sha256:literal, $size:literal) => {
        File {
            name: $name,
            url: concat!("https://huggingface.co/desert-ant-labs/uhm/resolve/fcbc8f364cef3e0bd3a54d1489d76ca433070dd5/", $name),
            sha256: $sha256,
            size: $size,
        }
    };
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

/// A file of Desert Ant's Who (fp16 Core ML), at the repo commit behind tag v0.3.4: the revision SDK 4.2.1 pins.
macro_rules! who_file {
    ($name:literal, $sha256:literal, $size:literal) => {
        File {
            name: $name,
            url: concat!("https://huggingface.co/desert-ant-labs/who/resolve/f026f4604f874e2cfe6b817e5123c2b64a56683d/", $name),
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
    // Core ML detector plus its type-labeler head, at the repo commit behind tag v1.2.0 (the revision SDK 4.2.1 pins).
    // Same non-LFS caveat as above for the small json/mil files.
    Model {
        id: "uhm",
        task: "filler",
        name: "Uhm",
        desc: "Finds \"uh\" and \"um\" in the audio itself. Experimental.",
        files: &[
            uhm_file!("uhm.mlmodelc/analytics/coremldata.bin", "0693408f429a19d9f6ae21b5dbbb1dceb06e1e499281b1c6c842cbef29195718", 243),
            uhm_file!("uhm.mlmodelc/coremldata.bin", "f87f7ff6605d389e853488363680654025f5c1765c93d282c95d2fec04cd6287", 991),
            uhm_file!("uhm.mlmodelc/metadata.json", "49f5c52b16f4230d48f64c4ab60835e28c66678bea163d0ca35b1a3f86c547ce", 2_806),
            uhm_file!("uhm.mlmodelc/model.mil", "2445325b0d767a85e432bd48a6e95cd178afc325224dd3eee4b3248eb7e7036a", 281_547),
            uhm_file!("uhm.mlmodelc/weights/weight.bin", "f463c73ff42cd2cc2fe5ad4954f0af1218468ca8c13e183ea9c7de529c84701a", 47_100_992),
            uhm_file!("UhmLabel.mlmodel", "234aada7598f99786efd3b05cdac783da408a3123fe12fb1dca5e52035d59c31", 12_950),
        ],
    },
    // Core ML files again, loaded only by the Desert Ant SDK (see who.rs). Names keep the repo's
    // `coreml-fp16/` folder, which is where the SDK looks. Hashes of the non-LFS files were computed from the download.
    Model {
        id: "who",
        task: "diarization",
        name: "Who",
        desc: "Default. Runs on the Neural Engine. Up to four speakers. Needs macOS 15.",
        files: &[
            who_file!("coreml-fp16/encoder_30000.mlmodelc/analytics/coremldata.bin", "2f2656633f626f87beba6e47e533baed3874d1eacd3228c2bf351326858cc53e", 243),
            who_file!("coreml-fp16/encoder_30000.mlmodelc/coremldata.bin", "a1b2a35fddb8755483ca4bd9cb3d51e15c79c1e9ce3b052e1e55f69ba75a444e", 399),
            who_file!("coreml-fp16/encoder_30000.mlmodelc/model.mil", "3adf9eb810955c207527c87868ff926b8594919d5ea2cc7e5a220d4ea2aa3499", 228_136),
            who_file!("coreml-fp16/encoder_30000.mlmodelc/weights/weight.bin", "998194432a81fef3c66350502c624cf844e62592223c25bf5deb93031f529bd1", 23_349_888),
            who_file!("coreml-fp16/encoder_6000.mlmodelc/analytics/coremldata.bin", "203e8dfd90815c1d197147d44cf9cc41da5031c5c0fe15398918023bb3c57722", 243),
            who_file!("coreml-fp16/encoder_6000.mlmodelc/coremldata.bin", "f3c6f8e3f44270e1d0672cb0ecce98de3a2de3715a0d1c2f13f6cb341f28fb76", 397),
            who_file!("coreml-fp16/encoder_6000.mlmodelc/model.mil", "22c7928cf61eb8594436075f1200f489232a5fc9ed61dc8b3fdfb6e7287418e6", 227_558),
            who_file!("coreml-fp16/encoder_6000.mlmodelc/weights/weight.bin", "fa34257f332b5ebfcd53d1461cbb87f76a02c5bb129a33703dcafd9a11477316", 20_834_688),
            who_file!("coreml-fp16/queries_30000.mlmodelc/analytics/coremldata.bin", "92335d6b04331041d4f255e5bafe7befc837a9f6d37d03fa4beeba25cb4cb347", 243),
            who_file!("coreml-fp16/queries_30000.mlmodelc/coremldata.bin", "979397a485b3c0e1c9a8dd6e4c8bffeed4f019b8323b722b3c586ff311f056b7", 504),
            who_file!("coreml-fp16/queries_30000.mlmodelc/model.mil", "6d2f360abe974b7bd9fce4e077735a005ff54d0640d23be50041768b6c75379a", 135_814),
            who_file!("coreml-fp16/queries_30000.mlmodelc/weights/weight.bin", "46687be0f958349341bc7e2bf84bda5e4d7365ff5056462a205d19f1f56f81fb", 13_019_136),
            who_file!("coreml-fp16/queries_6000.mlmodelc/analytics/coremldata.bin", "617e1a729e0a763826236a7a4cd1e2d6252aad96b34f052a9044c8e7f7c00800", 243),
            who_file!("coreml-fp16/queries_6000.mlmodelc/coremldata.bin", "76b9725564138b9741ac553e1d8fdd03987645e603124edd2fa0c575e706ccb4", 452),
            who_file!("coreml-fp16/queries_6000.mlmodelc/model.mil", "5f9a152aa7c01fc4ba2c8916e29d6ec581cb3b41df53afc44661b85be7c2336b", 134_015),
            who_file!("coreml-fp16/queries_6000.mlmodelc/weights/weight.bin", "46687be0f958349341bc7e2bf84bda5e4d7365ff5056462a205d19f1f56f81fb", 13_019_136),
            who_file!("coreml-fp16/voice_3256.mlmodelc/analytics/coremldata.bin", "874f2018ec09db2d185cb43c1c55a65d383b4b8d6f435d746e1c73045594fb48", 243),
            who_file!("coreml-fp16/voice_3256.mlmodelc/coremldata.bin", "ac5fb237dc7f59da6bb643f7de7ee98dafadd9b3c3f3770cd1df680abc53a442", 373),
            who_file!("coreml-fp16/voice_3256.mlmodelc/model.mil", "6eebc5e3608f69ebe755c51f7d7b98799e1310afb93df643a1c98cf00f9caf89", 57_727),
            who_file!("coreml-fp16/voice_3256.mlmodelc/weights/weight.bin", "b8c030fc8b0053f9274f4f54a87e9c575ce2df9dd207f4098a1ca96ec0dfa57f", 10_642_880),
            who_file!("coreml-fp16/who_embed.f16", "2f2cb55e4370c30b3d5f0401bf421e018fcc8633f5d3aef225375704c60a8c1c", 2_621_960),
            who_file!("coreml-fp16/who_mel_banks.f32", "730d76d5d2099d964c705c82c4556e2b10792d287666aa97edb0633f88f08ce0", 23_652),
            who_file!("coreml-fp16/who_meta.json", "cd2a2e311db7e7f6c0248a635b59b21fff5028837fef9ccc1ea5c715edd7e737", 3_954),
            who_file!("coreml-fp16/who_voice_banks.f32", "df33ff45ac021f5d014356b4e9ab4b9f91fadd836649962ee6e89eadfcd264e3", 82_248),
        ],
    },
    // NVIDIA's Nemotron-3 Diarization (Sortformer v3), exported for parakeet-rs 0.4, which no longer loads the v2.1 export.
    Model {
        id: "sortformer-v3",
        task: "diarization",
        name: "Sortformer v3",
        desc: "Open fallback from NVIDIA. Up to eight speakers.",
        files: &[File {
            name: "nemotron3_diar_v3.onnx",
            url: "https://huggingface.co/altunenes/parakeet-rs/resolve/b53b9101fa87271041773b5036aeb8db9d810d40/nemotron-3-diarization/nemotron3_diar_v3.onnx",
            sha256: "c32d6056f1ca52bdd50a74e1e7c66d91e5147bc29f19ffa4c39cbe5452397d15",
            size: 402_167_515,
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
