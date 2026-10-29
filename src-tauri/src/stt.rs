//! Which speech model dictation uses: the curated list plus the user's own models (a local path or
//! a Hugging Face repo), and what each runtime needs on disk before it can load.
use crate::{models, settings};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

pub const PARAKEET: &str = "parakeet-tdt-0.6b-v3";
pub const WHISPER: &str = "whisper-large-v3-turbo";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Parakeet,
    Whisper,
}

// Same names parakeet-rs looks for, int8 first so a repo with both downloads the small one.
const ENCODERS: [&str; 3] = ["encoder-model.int8.onnx", "encoder-model.onnx", "encoder.onnx"];
const DECODERS: [&str; 4] =
    ["decoder_joint-model.int8.onnx", "decoder_joint-model.onnx", "decoder_joint.onnx", "decoder-model.onnx"];
/// whisper.cpp model files start with this little-endian u32 ("ggml").
const GGML_MAGIC: [u8; 4] = 0x6767_6d6c_u32.to_le_bytes();

/// The files a Parakeet model is made of, found by `have`, or what is missing.
fn parakeet_names(have: impl Fn(&str) -> bool) -> Result<Vec<String>, String> {
    let find = |list: &[&str]| list.iter().find(|n| have(n)).map(|n| n.to_string());
    let (enc, dec) = (find(&ENCODERS), find(&DECODERS));
    let missing: Vec<&str> = [
        (!have("vocab.txt")).then_some("vocab.txt"),
        enc.is_none().then_some("an encoder (encoder-model.onnx)"),
        dec.is_none().then_some("a decoder (decoder_joint-model.onnx)"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !missing.is_empty() {
        return Err(format!("Not a Parakeet model: missing {}.", missing.join(", ")));
    }
    let (enc, dec) = (enc.unwrap(), dec.unwrap());
    let data = format!("{enc}.data"); // big fp32 encoders keep their weights beside the graph
    let mut names = vec!["vocab.txt".to_string(), enc];
    names.extend(have(&data).then_some(data));
    names.push(dec);
    Ok(names)
}

fn whisper_entry(path: &Path) -> Result<PathBuf, String> {
    let file = if path.is_dir() {
        let mut bins = fs::read_dir(path)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "bin"));
        match (bins.next(), bins.next()) {
            (Some(f), None) => f,
            (None, _) => return Err("No .bin model in that folder.".into()),
            _ => return Err("Several .bin models in that folder. Choose one file.".into()),
        }
    } else {
        path.to_path_buf()
    };
    let mut magic = [0u8; 4];
    fs::File::open(&file)
        .and_then(|mut f| f.read_exact(&mut magic))
        .map_err(|_| format!("Can't read {}.", file.display()))?;
    if magic != GGML_MAGIC {
        return Err("Not a whisper.cpp model. Use a ggml .bin file.".into());
    }
    Ok(file)
}

impl Runtime {
    pub fn label(self) -> &'static str {
        match self {
            Runtime::Parakeet => "Parakeet",
            Runtime::Whisper => "Whisper",
        }
    }

    /// What to hand the backend for a model installed at `path` (a folder for Parakeet, the .bin
    /// file for Whisper), or why it can't be used.
    pub fn entry(self, path: &Path) -> Result<PathBuf, String> {
        if !path.exists() {
            return Err(format!("Nothing at {}.", path.display()));
        }
        match self {
            Runtime::Parakeet => parakeet_names(|n| path.join(n).is_file()).map(|_| path.to_path_buf()),
            Runtime::Whisper => whisper_entry(path),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Source {
    Path(PathBuf),
    /// `file` is the .bin to take from a repo that has several (Whisper).
    Repo { id: String, file: Option<String> },
}

/// An absolute path (or `~/…`), `owner/name`, or `owner/name/file.bin`.
pub fn parse_source(input: &str) -> Result<Source, String> {
    let s = input.trim();
    if let Some(rest) = s.strip_prefix("~/") {
        let home = std::env::var_os("HOME").ok_or("Can't find your home folder.")?;
        return Ok(Source::Path(Path::new(&home).join(rest)));
    }
    if s.starts_with('/') {
        return Ok(Source::Path(PathBuf::from(s)));
    }
    let ok = |p: &&str| !p.is_empty() && !p.starts_with('.') && p.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() < 2 || parts.len() > 3 || !parts.iter().all(ok) {
        return Err("Enter a folder or file path (starting with / or ~), or a Hugging Face repo like owner/name.".into());
    }
    Ok(Source::Repo { id: parts[..2].join("/"), file: parts.get(2).map(|f| f.to_string()) })
}

/// The parts of Hugging Face's `/api/models/<repo>?blobs=true` answer that we use.
#[derive(Deserialize)]
pub struct Info {
    sha: String,
    siblings: Vec<Sibling>,
}

#[derive(Deserialize)]
struct Sibling {
    rfilename: String,
    size: Option<u64>,
    lfs: Option<Lfs>,
}

#[derive(Deserialize)]
struct Lfs {
    sha256: String,
}

pub fn fetch_repo(id: &str) -> Result<Info, String> {
    let url = format!("https://huggingface.co/api/models/{id}?blobs=true");
    match ureq::get(&url).timeout(Duration::from_secs(15)).call() {
        Ok(r) => serde_json::from_reader(r.into_reader()).map_err(|e| format!("Unexpected answer from Hugging Face: {e}")),
        // Hugging Face answers 401 for repos that don't exist as well as private ones.
        Err(ureq::Error::Status(401 | 404, _)) => Err(format!("Couldn't find {id} on Hugging Face. It may be private.")),
        Err(e) => Err(format!("Couldn't reach Hugging Face: {e}")),
    }
}

/// Which repo files to download, pinned to the commit with Hugging Face's checksums.
fn plan(runtime: Runtime, id: &str, file: Option<&str>, info: Info) -> Result<Vec<models::Remote>, String> {
    let top: Vec<&str> = info.siblings.iter().map(|s| s.rfilename.as_str()).filter(|n| !n.contains('/')).collect();
    let names = match (runtime, file) {
        (Runtime::Parakeet, Some(_)) => return Err("Parakeet takes a whole repo or folder, not one file.".into()),
        (Runtime::Parakeet, None) => parakeet_names(|n| top.contains(&n))?,
        (Runtime::Whisper, Some(f)) if !f.ends_with(".bin") => return Err("Whisper models are .bin files.".into()),
        (Runtime::Whisper, Some(f)) => {
            if !top.contains(&f) {
                return Err(format!("{f} isn't in {id}."));
            }
            vec![f.to_string()]
        }
        (Runtime::Whisper, None) => {
            let bins: Vec<&&str> = top.iter().filter(|n| n.ends_with(".bin")).collect();
            match bins[..] {
                [] => return Err(format!("{id} has no ggml .bin model.")),
                [one] => vec![one.to_string()],
                _ => return Err(format!("{id} has {} .bin models. Add one as {id}/{}.", bins.len(), bins[0])),
            }
        }
    };
    names
        .into_iter()
        .map(|name| {
            let s = info.siblings.iter().find(|s| s.rfilename == name).unwrap();
            Ok(models::Remote {
                url: format!("https://huggingface.co/{id}/resolve/{}/{name}", info.sha),
                sha256: s.lfs.as_ref().map(|l| l.sha256.clone()).unwrap_or_default(),
                size: s.size.ok_or_else(|| format!("Hugging Face didn't report a size for {name}."))?,
                name,
            })
        })
        .collect()
}

/// A model the user added: a local path (`files` empty) or a repo downloaded into the models folder.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Custom {
    pub runtime: Runtime,
    pub source: String,
    #[serde(default)]
    pub files: Vec<models::Remote>,
}

impl Custom {
    /// Validates `source` for `runtime`; a repo is looked up with `fetch` so the file list is known.
    pub fn new(runtime: Runtime, source: &str, fetch: impl FnOnce(&str) -> Result<Info, String>) -> Result<Self, String> {
        Ok(match parse_source(source)? {
            Source::Path(p) => {
                runtime.entry(&p)?;
                Custom { runtime, source: p.display().to_string(), files: vec![] }
            }
            Source::Repo { id, file } => {
                Custom { runtime, source: source.trim().to_string(), files: plan(runtime, &id, file.as_deref(), fetch(&id)?)? }
            }
        })
    }

    /// Stable and filesystem-safe: names the download folder and the radio button.
    pub fn id(&self) -> String {
        let h = Sha256::digest(format!("{:?}:{}", self.runtime, self.source));
        format!("custom-{}", h[..5].iter().map(|b| format!("{b:02x}")).collect::<String>())
    }

    pub fn local(&self) -> bool {
        self.source.starts_with('/')
    }

    pub fn name(&self) -> String {
        match self.source.rsplit_once('/') {
            Some((_, last)) if self.local() => last.to_string(),
            _ => self.source.clone(),
        }
    }

    pub fn dir(&self, root: &Path) -> PathBuf {
        if self.local() { PathBuf::from(&self.source) } else { root.join(self.id()) }
    }

    /// Runs `f` on this model viewed as a `models::Model`, so install/status/verify apply as they do to curated ones.
    pub fn model<R>(&self, f: impl FnOnce(&models::Model) -> R) -> R {
        let (id, name) = (self.id(), self.name());
        let desc = format!("Your {} model{}", self.runtime.label(), if self.local() { format!(" at {}", self.source) } else { " from Hugging Face".into() });
        let files: Vec<models::File> = self.files.iter().map(models::Remote::file).collect();
        f(&models::Model { id: &id, task: "stt", name: &name, desc: &desc, files: &files })
    }

    /// A local model is only ever checked, never fetched; a missing or wrong one says why.
    pub fn state(&self, root: &Path) -> (models::Status, Option<String>) {
        if !self.local() {
            return (self.model(|m| models::status(root, m)), None);
        }
        match self.runtime.entry(&self.dir(root)) {
            Ok(_) => (models::Status::Installed, None),
            Err(e) => (models::Status::Damaged, Some(e)),
        }
    }

    pub fn verify(&self, root: &Path) -> bool {
        if self.local() { self.state(root).1.is_none() } else { self.model(|m| models::verify(root, m)) }
    }
}

pub fn customs(db: &Connection) -> Vec<Custom> {
    settings::get(db, "custom_stt").and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
}

pub fn save(db: &Connection, all: &[Custom]) -> rusqlite::Result<()> {
    settings::set(db, "custom_stt", &serde_json::to_string(all).unwrap())
}

/// The runtime and path to load for the selected model `id`, or what to tell the user.
pub fn resolve(id: &str, root: &Path, all: &[Custom]) -> Result<(Runtime, PathBuf), String> {
    if let Some(c) = all.iter().find(|c| c.id() == id) {
        if !c.local() && c.state(root).0 != models::Status::Installed {
            return Err(format!("Download {} in Settings", c.name()));
        }
        return Ok((c.runtime, c.runtime.entry(&c.dir(root))?));
    }
    let m = models::MODELS.iter().find(|m| m.id == id).ok_or("Unknown speech model")?;
    let runtime = match id {
        PARAKEET => Runtime::Parakeet,
        WHISPER => Runtime::Whisper,
        _ => return Err(format!("{} can't dictate yet", m.name)),
    };
    if models::status(root, m) != models::Status::Installed {
        return Err("Download a speech model in Settings".into());
    }
    Ok((runtime, runtime.entry(&root.join(id))?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("myna-stt-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn touch(dir: &Path, names: &[&str]) {
        names.iter().for_each(|n| fs::write(dir.join(n), b"x").unwrap());
    }

    fn ggml(path: &Path) {
        fs::write(path, [&GGML_MAGIC[..], b"rest"].concat()).unwrap();
    }

    /// A trimmed copy of what Hugging Face answers for the Parakeet ONNX repo.
    fn listing(files: &[(&str, u64, Option<&str>)]) -> Info {
        let siblings: Vec<_> = files
            .iter()
            .map(|(n, size, lfs)| match lfs {
                Some(h) => format!(r#"{{"rfilename":"{n}","size":{size},"lfs":{{"sha256":"{h}","size":{size}}}}}"#),
                None => format!(r#"{{"rfilename":"{n}","size":{size}}}"#),
            })
            .collect();
        serde_json::from_str(&format!(r#"{{"sha":"abc123","siblings":[{}]}}"#, siblings.join(","))).unwrap()
    }

    fn parakeet_repo() -> Info {
        listing(&[
            ("README.md", 10, None),
            ("encoder-model.onnx", 40, Some("aa")),
            ("encoder-model.int8.onnx", 650, Some("bb")),
            ("decoder_joint-model.int8.onnx", 18, Some("cc")),
            ("vocab.txt", 9, None),
        ])
    }

    #[test]
    fn sources_are_told_apart() {
        assert_eq!(parse_source(" /Users/a/m.bin "), Ok(Source::Path("/Users/a/m.bin".into())));
        let home = std::env::var("HOME").unwrap();
        assert_eq!(parse_source("~/models/x"), Ok(Source::Path(Path::new(&home).join("models/x"))));
        assert_eq!(parse_source("istupakov/parakeet-v3.onnx"), Ok(Source::Repo { id: "istupakov/parakeet-v3.onnx".into(), file: None }));
        assert_eq!(
            parse_source("ggerganov/whisper.cpp/ggml-tiny.bin"),
            Ok(Source::Repo { id: "ggerganov/whisper.cpp".into(), file: Some("ggml-tiny.bin".into()) })
        );
        for bad in ["", "models", "relative/dir/with/many", "a/../b", "a/b c", ".hidden/x", "a//b", "https://huggingface.co/a/b"] {
            assert!(parse_source(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn parakeet_folder_must_have_all_its_files() {
        let dir = tmp("pk");
        assert!(Runtime::Parakeet.entry(&dir.join("nope")).unwrap_err().starts_with("Nothing at"));
        touch(&dir, &["encoder-model.int8.onnx"]);
        let err = Runtime::Parakeet.entry(&dir).unwrap_err();
        assert!(err.contains("vocab.txt") && err.contains("decoder") && !err.contains("encoder"), "{err}");
        touch(&dir, &["vocab.txt", "decoder_joint-model.int8.onnx"]);
        assert_eq!(Runtime::Parakeet.entry(&dir), Ok(dir.clone()));
    }

    #[test]
    fn whisper_needs_a_ggml_file() {
        let dir = tmp("wh");
        let (good, other) = (dir.join("ggml-tiny.bin"), dir.join("notes.bin"));
        assert!(Runtime::Whisper.entry(&dir).unwrap_err().contains("No .bin"));
        fs::write(&other, b"not a model").unwrap();
        assert!(Runtime::Whisper.entry(&other).unwrap_err().contains("Not a whisper.cpp model"));
        fs::remove_file(&other).unwrap();
        ggml(&good);
        assert_eq!(Runtime::Whisper.entry(&good), Ok(good.clone()));
        assert_eq!(Runtime::Whisper.entry(&dir), Ok(good.clone()), "a folder with one .bin resolves to it");
        ggml(&other);
        assert!(Runtime::Whisper.entry(&dir).unwrap_err().contains("Several"));
    }

    #[test]
    fn repo_plan_picks_the_small_files_with_checksums() {
        let files = plan(Runtime::Parakeet, "o/r", None, parakeet_repo()).unwrap();
        let got: Vec<_> = files.iter().map(|f| (f.name.as_str(), f.sha256.as_str(), f.size)).collect();
        assert_eq!(got, [("vocab.txt", "", 9), ("encoder-model.int8.onnx", "bb", 650), ("decoder_joint-model.int8.onnx", "cc", 18)]);
        assert_eq!(files[0].url, "https://huggingface.co/o/r/resolve/abc123/vocab.txt");
        assert!(plan(Runtime::Parakeet, "o/r", Some("x.bin"), parakeet_repo()).is_err());
        // fp32 export: the weights file beside the graph comes along.
        let fp32 = listing(&[("encoder-model.onnx", 4, Some("a")), ("encoder-model.onnx.data", 99, Some("b")), ("decoder_joint-model.onnx", 5, Some("c")), ("vocab.txt", 1, None)]);
        let names: Vec<_> = plan(Runtime::Parakeet, "o/r", None, fp32).unwrap().into_iter().map(|f| f.name).collect();
        assert!(names.contains(&"encoder-model.onnx.data".to_string()));
        // Not a Parakeet repo.
        let err = plan(Runtime::Parakeet, "o/r", None, listing(&[("ggml-tiny.bin", 5, Some("a"))])).unwrap_err();
        assert!(err.contains("vocab.txt"), "{err}");
    }

    #[test]
    fn whisper_repo_plan_needs_one_unambiguous_bin() {
        let one = listing(&[("README.md", 1, None), ("ggml-tiny.bin", 75, Some("t"))]);
        assert_eq!(plan(Runtime::Whisper, "o/r", None, one).unwrap()[0].name, "ggml-tiny.bin");
        let many = || listing(&[("ggml-base.bin", 1, Some("b")), ("ggml-tiny.bin", 2, Some("t")), ("sub/ggml-x.bin", 3, Some("x"))]);
        let err = plan(Runtime::Whisper, "o/r", None, many()).unwrap_err();
        assert!(err.contains("2 .bin models") && err.contains("o/r/ggml-base.bin"), "{err}");
        assert_eq!(plan(Runtime::Whisper, "o/r", Some("ggml-tiny.bin"), many()).unwrap()[0].sha256, "t");
        assert!(plan(Runtime::Whisper, "o/r", Some("missing.bin"), many()).unwrap_err().contains("isn't in"));
        assert!(plan(Runtime::Whisper, "o/r", Some("sub"), many()).is_err());
        assert!(plan(Runtime::Whisper, "o/r", None, listing(&[("vocab.txt", 1, None)])).unwrap_err().contains("no ggml"));
    }

    #[test]
    fn custom_models_validate_before_they_are_added() {
        let dir = tmp("custom");
        let never = |_: &str| -> Result<Info, String> { panic!("a local path needs no network") };
        assert!(Custom::new(Runtime::Whisper, &dir.display().to_string(), never).is_err());
        ggml(&dir.join("m.bin"));
        let local = Custom::new(Runtime::Whisper, &dir.join("m.bin").display().to_string(), never).unwrap();
        assert!(local.local() && local.files.is_empty());
        assert_eq!(local.name(), "m.bin");

        let repo = Custom::new(Runtime::Parakeet, "o/r", |id| {
            assert_eq!(id, "o/r");
            Ok(parakeet_repo())
        })
        .unwrap();
        assert!(!repo.local() && repo.files.len() == 3);
        assert_eq!(repo.name(), "o/r");
        assert_eq!(Custom::new(Runtime::Parakeet, "o/r", |_| Err("Couldn't find o/r on Hugging Face.".into())).unwrap_err(), "Couldn't find o/r on Hugging Face.");
        // Same source under the other runtime is a different model.
        assert_ne!(repo.id(), Custom { runtime: Runtime::Whisper, ..repo.clone() }.id());
        assert!(repo.id().starts_with("custom-") && !repo.id().contains('/'));
    }

    #[test]
    fn selection_resolves_to_a_runtime_and_path() {
        let root = tmp("resolve");
        let db = settings::open(&root.join("t.db")).unwrap();
        assert_eq!(resolve("nope", &root, &[]).unwrap_err(), "Unknown speech model");
        assert_eq!(resolve(PARAKEET, &root, &[]).unwrap_err(), "Download a speech model in Settings");
        assert_eq!(resolve(WHISPER, &root, &[]).unwrap_err(), "Download a speech model in Settings");
        assert!(resolve("sortformer-4spk-v2.1", &root, &[]).unwrap_err().contains("can't dictate yet"));

        // A repo model: absent until downloaded, then loadable from its own folder.
        let mut repo = Custom::new(Runtime::Whisper, "o/r", |_| Ok(listing(&[("ggml-t.bin", 8, Some("h"))]))).unwrap();
        let id = repo.id();
        assert_eq!(resolve(&id, &root, std::slice::from_ref(&repo)).unwrap_err(), "Download o/r in Settings");
        fs::create_dir_all(root.join(&id)).unwrap();
        ggml(&root.join(&id).join("ggml-t.bin"));
        assert_eq!(resolve(&id, &root, std::slice::from_ref(&repo)), Ok((Runtime::Whisper, root.join(&id).join("ggml-t.bin"))));

        // A local model needs no download and is re-checked on every use.
        let path = root.join("mine.bin");
        ggml(&path);
        repo = Custom::new(Runtime::Whisper, &path.display().to_string(), |_| unreachable!()).unwrap();
        let all = [repo];
        assert_eq!(resolve(&all[0].id(), &root, &all), Ok((Runtime::Whisper, path.clone())));
        fs::remove_file(&path).unwrap();
        assert!(resolve(&all[0].id(), &root, &all).unwrap_err().starts_with("Nothing at"));

        // Stored in settings and read back.
        assert!(customs(&db).is_empty());
        save(&db, &all).unwrap();
        assert_eq!(customs(&db), all);
    }
}
