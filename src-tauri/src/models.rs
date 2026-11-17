//! The bundled catalog of on-device models (models/local.json), what's installed, and the active model per task.

use crate::storage;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct File {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    /// Set when the file isn't in the model's Hugging Face repo but is one member of a .tgz at a URL.
    #[serde(default)]
    pub archive: Option<Archive>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Archive {
    pub url: String,
    pub size: u64,
    pub sha256: String,
    /// The path inside the archive of the file to keep.
    pub member: String,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct Model {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub acc: u8,
    #[serde(default)]
    pub spd: u8,
    #[serde(default)]
    pub default: bool,
    pub size: u64,
    /// 0 = any macOS Myna runs on.
    #[serde(default)]
    pub min_macos: u32,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub revision: String,
    /// Empty for `sdk` models (their SDK fetches and verifies its own weights) and Apple Speech.
    pub files: Vec<File>,
    /// Built into macOS (Apple Speech): macOS downloads and owns its model.
    #[serde(default)]
    pub system: bool,
    /// Its SDK downloads and verifies its own weights into `models/<id>/` (Desert Ant Voz).
    #[serde(default)]
    pub sdk: bool,
    /// Runs on ONNX Runtime, which is downloaded with it and shared with the other models that need it.
    #[serde(default)]
    pub runtime: bool,
    #[serde(default)]
    pub installed: bool,
    /// Why it can't be picked on this Mac, e.g. "Needs macOS 15". None if it can.
    #[serde(default)]
    pub needs: Option<String>,
    /// A model added under Advanced: the runtime that runs it, one of `ENGINES`. Empty for the catalog's.
    #[serde(default)]
    pub engine: String,
    /// A custom model the user keeps on this Mac: its file or folder, used in place. Empty when it's in `models/<id>/`.
    #[serde(default)]
    pub path: String,
    /// A model added under Advanced has loaded here: until it has, it can't be picked and offers Download, which checks it.
    #[serde(default)]
    pub checked: bool,
}

/// What macOS says about Apple Speech on this Mac.
#[derive(Deserialize, Default, Clone)]
pub struct System {
    /// The Mac's language, e.g. "English (India)". None if Apple Speech can't hear it.
    pub language: Option<String>,
    pub installed: bool,
}

#[derive(Deserialize)]
struct Catalog {
    /// ONNX Runtime, a dependency of the models with `runtime` set. Not a model anyone picks.
    runtime: Model,
    stt: Vec<Model>,
    diar: Vec<Model>,
}

#[derive(Serialize)]
pub struct Task {
    pub id: &'static str,
    pub models: Vec<Model>,
    /// None only if no model in the task runs on this macOS.
    pub active: Option<String>,
}

fn catalog() -> Catalog {
    serde_json::from_str(include_str!("../../models/local.json")).expect("models/local.json is valid")
}

/// Meeting notes' own speech-to-text pick, from the same catalog and downloads as voice typing's. With no row saved,
/// meetings use the `stt` pick, so installs from before it (and older builds, which never read it) behave as before.
pub const MEETING_STT: &str = "meeting_stt";

/// The catalog's models for `task`, then the ones added under Advanced.
fn task_models(conn: &Connection, task: &str) -> Result<Option<Vec<Model>>, String> {
    let c = catalog();
    let (mut models, kind) = match task {
        "stt" | MEETING_STT => (c.stt, "stt"),
        "diar" => (c.diar, "diar"),
        _ => return Ok(None),
    };
    models.extend(customs(conn)?.into_iter().filter(|m| engine_task(&m.engine) == Some(kind)));
    Ok(Some(models))
}

pub fn needs(m: &Model, macos: u32, sys: &System) -> Option<String> {
    if macos < m.min_macos {
        return Some(format!("Needs macOS {}", m.min_macos));
    }
    (m.system && sys.language.is_none()).then(|| "Needs a language Apple supports".into())
}

pub fn find(id: &str) -> Option<Model> {
    all().into_iter().find(|m| m.id == id)
}

/// A catalog model or one added under Advanced.
pub fn get(conn: &Connection, id: &str) -> Result<Model, String> {
    find(id).or(customs(conn)?.into_iter().find(|m| m.id == id)).ok_or_else(|| format!("Unknown model {id}"))
}

/// The catalog's models and the ones added under Advanced.
pub fn everything(conn: &Connection) -> Result<Vec<Model>, String> {
    Ok(all().into_iter().chain(customs(conn)?).collect())
}

pub fn all() -> Vec<Model> {
    let c = catalog();
    c.stt.into_iter().chain(c.diar).collect()
}

pub fn runtime() -> Model {
    catalog().runtime
}

/// Where the ONNX Runtime library is once it's downloaded.
pub fn runtime_dylib(models_dir: &Path) -> PathBuf {
    let r = runtime();
    models_dir.join(&r.id).join(&r.files[0].path)
}

/// Every catalog file is in `models/<id>/` at its catalog size. The sha256 is checked when it's downloaded.
/// A custom model on this Mac is there while its file or folder is.
pub fn is_installed(m: &Model, models_dir: &Path) -> bool {
    if !m.path.is_empty() {
        return Path::new(&m.path).exists();
    }
    !m.files.is_empty()
        && m.files.iter().all(|f| {
            std::fs::metadata(models_dir.join(&m.id).join(&f.path)).is_ok_and(|md| md.len() == f.size)
        })
}

pub fn list(conn: &Connection, models_dir: &Path, macos: u32, sys: &System) -> Result<Vec<Task>, String> {
    let rt = runtime();
    let rt_there = is_installed(&rt, models_dir);
    let mut tasks: Vec<Task> = Vec::new();
    // In this order: the Settings window reads them by position, and meeting_stt falls back to the stt pick.
    for id in ["stt", "diar", MEETING_STT] {
        tasks.push({
            let mut models = task_models(conn, id)?.unwrap();
            for m in &mut models {
                m.installed = if m.system {
                    sys.installed
                } else if m.sdk {
                    let dir = models_dir.join(&m.id);
                    if m.id == "who" { crate::who::installed(&dir) } else { crate::voz::installed(&dir) }
                } else { is_installed(m, models_dir) && (rt_there || !m.runtime) && (m.engine.is_empty() || m.checked) };
                if m.runtime && !rt_there {
                    m.size += rt.size; // one combined size
                }
                m.needs = needs(m, macos, sys).or(m.needs.take()); // a custom model that didn't load keeps saying so
                if let (true, Some(lang)) = (m.system, &sys.language) {
                    m.desc = format!("{} Listens in {lang}.", m.desc);
                }
            }
            let saved: Option<String> = conn
                .query_row("SELECT model_id FROM active_model WHERE task = ?1", [id], |r| r.get(0))
                .optional()
                .map_err(|e| format!("Couldn't read the chosen models: {e}"))?;
            let usable = |want: &dyn Fn(&Model) -> bool| {
                models.iter().find(|m| m.needs.is_none() && (m.engine.is_empty() || m.checked) && want(m)).map(|m| m.id.clone())
            };
            // The recommended model once it's downloaded; until then Apple Speech, so nobody waits on a download.
            let fallback = (id == MEETING_STT).then(|| tasks[0].active.clone()).flatten();
            let active = usable(&|m| Some(&m.id) == saved.as_ref())
                .or(fallback)
                .or_else(|| usable(&|m| m.default && m.installed))
                .or_else(|| usable(&|m| m.system))
                .or_else(|| usable(&|m| m.default))
                .or_else(|| usable(&|_| true));
            Task { id, models, active }
        });
    }
    Ok(tasks)
}

pub fn select(conn: &Connection, task: &str, id: &str, macos: u32, sys: &System) -> Result<(), String> {
    let models = task_models(conn, task)?.ok_or_else(|| format!("Unknown task {task}"))?;
    let m = models.iter().find(|m| m.id == id).ok_or_else(|| format!("Unknown model {id}"))?;
    if let Some(n) = needs(m, macos, sys).or(m.needs.clone()) {
        return Err(format!("{}: {n}.", m.name));
    }
    if !m.engine.is_empty() && !m.checked {
        return Err(format!("{} hasn't loaded yet. Download it to check it.", m.name));
    }
    conn.execute(
        "INSERT INTO active_model(task, model_id) VALUES (?1, ?2) ON CONFLICT(task) DO UPDATE SET model_id = ?2",
        [task, id],
    )
    .map_err(|e| format!("Couldn't save the chosen model: {e}"))?;
    Ok(())
}

/// Drops `id` as any task's saved pick (voice typing's and meeting notes' both), so a deleted model hands over to the task's default.
pub fn forget(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM active_model WHERE model_id = ?1", [id]).map_err(|e| format!("Couldn't update the chosen models: {e}"))?;
    Ok(())
}

/// The runtimes a model added under Advanced can run on: id, task, name.
pub const ENGINES: [(&str, &str, &str); 4] =
    [("whisper", "stt", "Whisper"), ("parakeet-tdt", "stt", "Parakeet TDT"), ("parakeet-unified", "stt", "Parakeet Unified"), ("sortformer", "diar", "Sortformer")];

pub fn engine_task(engine: &str) -> Option<&'static str> {
    ENGINES.iter().find(|e| e.0 == engine).map(|e| e.1)
}

/// Models added under Advanced, each saved whole as JSON. Kept as JSON values, so one a newer Myna added survives an edit here.
const CUSTOM: &str = "custom_models";

fn saved(conn: &Connection) -> Result<Vec<serde_json::Value>, String> {
    let s = storage::read_text(conn, CUSTOM).map_err(|e| format!("Couldn't read your added models: {e}"))?;
    Ok(s.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
}

fn save(conn: &Connection, all: &[serde_json::Value]) -> Result<(), String> {
    storage::write_text(conn, CUSTOM, &serde_json::Value::from(all).to_string()).map_err(|e| format!("Couldn't save your added models: {e}"))
}

/// The models added under Advanced, leaving out any a newer Myna added for a runtime this one lacks.
pub fn customs(conn: &Connection) -> Result<Vec<Model>, String> {
    Ok(saved(conn)?.into_iter().filter_map(|v| serde_json::from_value::<Model>(v).ok()).filter(|m| engine_task(&m.engine).is_some()).collect())
}

/// Adds a model under Advanced from `source`: a file or folder on this Mac (starting with / or ~), used where it is, or a
/// Hugging Face repo, downloaded later like a catalog model. `get` reads a Hugging Face API address. A model that's
/// already on this Mac with what it needs must `load`, or it isn't added; a repo is loaded once it's downloaded.
/// Holds the database only to save, not while it asks Hugging Face or loads.
pub fn add(
    db: &std::sync::Mutex<Connection>,
    models_dir: &Path,
    engine: &str,
    source: &str,
    get: &dyn Fn(&str) -> Result<String, String>,
    load: &dyn Fn(&Model) -> Result<(), String>,
) -> Result<Model, String> {
    let label = ENGINES.iter().find(|e| e.0 == engine).ok_or_else(|| format!("Unknown runtime {engine}"))?.2;
    let source = source.trim();
    let mut m = if source.starts_with('/') || source.starts_with('~') {
        let path = match source.strip_prefix('~') {
            Some(rest) => format!("{}{rest}", std::env::var("HOME").unwrap_or_default()),
            None => source.to_string(),
        };
        if !Path::new(&path).exists() {
            return Err(format!("There's nothing at {path}."));
        }
        let name = Path::new(&path).file_name().map_or(path.clone(), |n| n.to_string_lossy().into_owned());
        Model { name, vendor: "On this Mac".into(), desc: format!("{label} · {path}"), path, ..Default::default() }
    } else {
        let (repo, rev, at) = parse_hf(source).ok_or("Paste a path that starts with / or ~, or a Hugging Face repo like ggerganov/whisper.cpp.")?;
        let url = format!("{}/api/models/{repo}/revision/{rev}?blobs=true", crate::download::HF);
        let body = get(&url).map_err(|e| format!("Couldn't find {repo} on Hugging Face ({e}). Check the name, and that the repo is public."))?;
        let info: HfInfo = serde_json::from_str(&body).map_err(|e| format!("Hugging Face sent an answer Myna can't read for {repo}: {e}"))?;
        if !safe(&info.sha) {
            return Err(format!("Hugging Face sent an answer Myna can't read for {repo}."));
        }
        let files = pick(engine, label, &repo, &info.siblings, &at)?;
        let name = files[0].path.rsplit('/').next().unwrap_or_default().to_string();
        let name = if files.len() == 1 { name } else { files[0].path.rsplit('/').nth(1).unwrap_or(repo.rsplit('/').next().unwrap()).to_string() };
        let desc = format!("{label} · {repo}{}{at}", if at.is_empty() { "" } else { "/" });
        Model { name, vendor: "Hugging Face".into(), desc, size: files.iter().map(|f| f.size).sum(), repo, revision: info.sha, files, ..Default::default() }
    };
    m.engine = engine.into();
    m.runtime = engine != "whisper";
    // An ONNX model on this Mac waits for ONNX Runtime, which downloads like a repo.
    m.installed = !m.path.is_empty() && (!m.runtime || is_installed(&runtime(), models_dir));
    if m.installed {
        load(&m)?;
    }
    m.checked = m.installed;
    let conn = db.lock().unwrap();
    let all = saved(&conn)?;
    let paths = |m: &Model| m.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
    if let Some(o) = customs(&conn)?.into_iter().find(|o| (&o.engine, &o.path, &o.repo, paths(o)) == (&m.engine, &m.path, &m.repo, paths(&m))) {
        return Err(format!("{} is already in the list.", o.name));
    }
    let base = format!("custom-{}", m.name.to_lowercase().split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join("-"));
    let taken = |id: &str| find(id).is_some() || all.iter().any(|v| v["id"] == id) || models_dir.join(id).exists();
    m.id = (1..).map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") }).find(|id| !taken(id)).unwrap();
    save(&conn, &[all, vec![serde_json::to_value(&m).unwrap()]].concat())?;
    Ok(m)
}

/// Takes a model added under Advanced off the list. Its files are the caller's to delete: never a file or folder of the user's.
pub fn remove_custom(conn: &Connection, id: &str) -> Result<(), String> {
    let mut all = saved(conn)?;
    all.retain(|v| v["id"] != id);
    save(conn, &all)
}

/// A model added under Advanced that didn't load says why in its row, and can't be picked.
pub fn mark_broken(conn: &Connection, id: &str, why: &str) -> Result<(), String> {
    let mut all = saved(conn)?;
    for v in all.iter_mut().filter(|v| v["id"] == id) {
        if v["was"].is_null() {
            v["was"] = v["desc"].clone(); // what `desc` said before the reason replaced it
        }
        v["needs"] = "Won't load".into();
        v["desc"] = why.into();
        v["checked"] = false.into();
    }
    save(conn, &all)
}

/// A model added under Advanced loaded: it can be picked, and a mark from an earlier failed load is gone.
pub fn mark_checked(conn: &Connection, id: &str) -> Result<(), String> {
    let mut all = saved(conn)?;
    for v in all.iter_mut().filter(|v| v["id"] == id) {
        if let Some(was) = v.as_object_mut().and_then(|o| o.remove("was")) {
            v["desc"] = was;
        }
        v["needs"] = serde_json::Value::Null;
        v["checked"] = true.into();
    }
    save(conn, &all)
}

#[derive(Deserialize)]
struct HfInfo {
    sha: String,
    siblings: Vec<HfFile>,
}

#[derive(Deserialize)]
struct HfFile {
    rfilename: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    lfs: Option<HfLfs>,
}

#[derive(Deserialize)]
struct HfLfs {
    sha256: String,
}

/// One part of a repo name, revision or path: nothing that could climb out of the models folder.
fn safe(p: &str) -> bool {
    !p.is_empty() && p != "." && p != ".." && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

/// A Hugging Face repo as `org/name` or a huggingface.co link: to the repo, a folder in it (`tree/<revision>/<folder>`)
/// or a file (`blob/` or `resolve/<revision>/<file>`). Gives the repo, the revision and the path in it ("" for the top).
pub fn parse_hf(s: &str) -> Option<(String, String, String)> {
    let s = s.trim().split(['?', '#']).next()?.trim_end_matches('/');
    let rest = ["https://huggingface.co/", "http://huggingface.co/", "huggingface.co/"].iter().find_map(|p| s.strip_prefix(p)).unwrap_or(s);
    match rest.split('/').collect::<Vec<_>>().as_slice() {
        [org, name] if safe(org) && safe(name) => Some((format!("{org}/{name}"), "main".into(), String::new())),
        [org, name, "tree" | "blob" | "resolve", rev, path @ ..] if [org, name, rev].iter().all(|p| safe(p)) && path.iter().all(|p| safe(p)) => {
            Some((format!("{org}/{name}"), rev.to_string(), path.join("/")))
        }
        _ => None,
    }
}

/// The files `engine` needs from a repo's listing. `at` is one file, a folder to look in (not below it), or "" for the top.
fn pick(engine: &str, label: &str, repo: &str, list: &[HfFile], at: &str) -> Result<Vec<File>, String> {
    let list: Vec<&HfFile> = list.iter().filter(|f| f.rfilename.split('/').all(safe)).collect();
    let folder = |p: &str| p.rsplit_once('/').map_or("", |(d, _)| d).to_string();
    let one = list.iter().copied().find(|f| f.rfilename == at);
    let dir = if one.is_some() { folder(at) } else { at.to_string() };
    let place = if dir.is_empty() { repo.to_string() } else { format!("{repo}/{dir}") };
    let here: Vec<&HfFile> = list.iter().copied().filter(|f| folder(&f.rfilename) == dir).collect();
    let file = |f: &HfFile| File { path: f.rfilename.clone(), size: f.size, sha256: f.lfs.as_ref().map_or(String::new(), |l| l.sha256.clone()), archive: None };
    if engine == "whisper" || engine == "sortformer" {
        let ext = if engine == "whisper" { ".bin" } else { ".onnx" };
        if let Some(f) = one {
            return if at.ends_with(ext) { Ok(vec![file(f)]) } else { Err(format!("{at} isn't a {label} model (a {ext} file).")) };
        }
        let found: Vec<_> = here.iter().filter(|f| f.rfilename.ends_with(ext)).collect();
        return match found.len() {
            0 => Err(format!("There's no {label} model (a {ext} file) in {place}.")),
            1 => Ok(vec![file(found[0])]),
            n => Err(format!("{place} has {n} {label} models. Paste the link to the one you want.")),
        };
    }
    // Parakeet: the encoder, the decoder and the vocabulary, by the names parakeet-rs looks for. The int8 export is a quarter of the size.
    let named = |n: &str| here.iter().copied().find(|f| f.rfilename.rsplit('/').next() == Some(n));
    let first = |what: &str, names: &[&str]| names.iter().find_map(|n| named(n)).ok_or_else(|| format!("There's no {label} {what} in {place}."));
    let enc = first("encoder", &["encoder-model.int8.onnx", "encoder.int8.onnx", "encoder-model.onnx", "encoder.onnx"])?;
    let dec = first("decoder", &["decoder_joint-model.int8.onnx", "decoder_joint.int8.onnx", "decoder_joint-model.onnx", "decoder_joint.onnx", "decoder-model.onnx"])?;
    let vocab = if engine == "parakeet-tdt" { first("vocab.txt", &["vocab.txt"])? } else { first("tokenizer.model", &["tokenizer.model"])? };
    let mut out = Vec::new();
    for f in [enc, dec] {
        out.push(file(f));
        // Weights over 2 GB sit beside the .onnx file.
        out.extend(named(&format!("{}.data", f.rfilename.rsplit('/').next().unwrap())).map(file));
    }
    out.push(file(vocab));
    Ok(out)
}

/// Seconds the speech-to-text model stays loaded after its last use; 0 unloads right away, -1 never.
pub const UNLOAD_CHOICES: [i64; 6] = [0, 60, 300, 900, 3600, -1];
const UNLOAD_DEFAULT: i64 = 300;

pub fn unload_after(conn: &Connection) -> Result<i64, String> {
    let saved = storage::read_text(conn, "stt_unload_secs").map_err(|e| format!("Couldn't read when to unload the model: {e}"))?;
    Ok(saved.and_then(|v| v.parse().ok()).filter(|v| UNLOAD_CHOICES.contains(v)).unwrap_or(UNLOAD_DEFAULT))
}

pub fn set_unload_after(conn: &Connection, secs: i64) -> Result<(), String> {
    if !UNLOAD_CHOICES.contains(&secs) {
        return Err(format!("{secs} s isn't one of the choices."));
    }
    storage::write_text(conn, "stt_unload_secs", &secs.to_string()).map_err(|e| format!("Couldn't save when to unload the model: {e}"))
}

/// Major macOS version, e.g. 15. 0 if unknown, which hides the macOS 15+ models rather than letting them fail later.
pub fn macos_major() -> u32 {
    std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok()?.split('.').next()?.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Connection) {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        (tmp, conn)
    }

    /// macOS 26+ in a language Apple Speech hears.
    fn english(installed: bool) -> System {
        System { language: Some("English (India)".into()), installed }
    }

    /// Voice typing's and diarization's picks.
    fn active(conn: &Connection, dir: &Path, macos: u32) -> Vec<Option<String>> {
        list(conn, dir, macos, &english(true)).unwrap().into_iter().take(2).map(|t| t.active).collect()
    }

    /// Voice typing's and meeting notes' speech-to-text picks.
    fn both(conn: &Connection, dir: &Path) -> (Option<String>, Option<String>) {
        let t = list(conn, dir, 26, &english(true)).unwrap();
        assert_eq!(t.iter().map(|t| t.id).collect::<Vec<_>>(), ["stt", "diar", MEETING_STT]);
        (t[0].active.clone(), t[2].active.clone())
    }

    #[test]
    fn meeting_notes_use_the_voice_typing_pick_until_they_have_their_own() {
        let (tmp, conn) = db();
        let w = || Some("whisper-large-v3-turbo".to_string());
        assert_eq!(both(&conn, tmp.path()), (Some("apple-speech".into()), Some("apple-speech".into())), "an install from before: the same model");
        select(&conn, "stt", "whisper-large-v3-turbo", 26, &english(true)).unwrap();
        assert_eq!(both(&conn, tmp.path()), (w(), w()), "follows voice typing while it has no row");
        select(&conn, MEETING_STT, "parakeet-tdt-v3", 26, &english(true)).unwrap();
        select(&conn, "stt", "apple-speech", 26, &english(true)).unwrap();
        assert_eq!(both(&conn, tmp.path()), (Some("apple-speech".into()), Some("parakeet-tdt-v3".into())), "separate picks");
        assert!(select(&conn, MEETING_STT, "sortformer", 26, &english(true)).is_err(), "speech-to-text models only");
    }

    #[test]
    fn deleting_a_model_forgets_it_for_voice_typing_and_meeting_notes() {
        let (tmp, conn) = db();
        select(&conn, "stt", "whisper-large-v3-turbo", 26, &english(true)).unwrap();
        select(&conn, MEETING_STT, "whisper-large-v3-turbo", 26, &english(true)).unwrap();
        forget(&conn, "whisper-large-v3-turbo").unwrap();
        assert_eq!(both(&conn, tmp.path()), (Some("apple-speech".into()), Some("apple-speech".into())));
        // A meeting pick of a deleted model falls back to voice typing's, not to the catalog default.
        select(&conn, "stt", "parakeet-tdt-v3", 26, &english(true)).unwrap();
        select(&conn, MEETING_STT, "whisper-large-v3-turbo", 26, &english(true)).unwrap();
        forget(&conn, "whisper-large-v3-turbo").unwrap();
        assert_eq!(both(&conn, tmp.path()).1.as_deref(), Some("parakeet-tdt-v3"));
    }

    #[test]
    fn unload_time_defaults_to_5_minutes_and_saves_only_listed_choices() {
        let (_tmp, conn) = db();
        assert_eq!(unload_after(&conn), Ok(300));
        set_unload_after(&conn, -1).unwrap();
        assert_eq!(unload_after(&conn), Ok(-1));
        assert!(set_unload_after(&conn, 7).is_err());
        assert_eq!(unload_after(&conn), Ok(-1));
    }

    #[test]
    fn catalog_is_well_formed() {
        for (task, models) in [("stt", catalog().stt), ("diar", catalog().diar)] {
            assert_eq!(models.iter().filter(|m| m.default).count(), 1, "{task}: one default");
            for m in &models {
                assert!((1..=5).contains(&m.acc) && (1..=5).contains(&m.spd), "{}", m.id);
                // One line in a model row; "Recommended" comes from `default`, not the text.
                assert!(m.desc.chars().count() <= 90 && !m.desc.contains("Recommended"), "{}: desc", m.id);
                assert!(m.system || m.revision.len() == 40, "{}: pinned commit", m.id);
                if !m.files.is_empty() {
                    assert_eq!(m.files.iter().map(|f| f.size).sum::<u64>(), m.size, "{}: size", m.id);
                }
                for f in &m.files {
                    assert!(f.sha256.len() == 64 && f.sha256.bytes().all(|b| b.is_ascii_hexdigit()), "{}", f.path);
                }
            }
        }
    }

    #[test]
    fn fresh_install_lists_everything_uninstalled_with_defaults() {
        let (tmp, conn) = db();
        let tasks = list(&conn, &tmp.path().join("models"), 15, &english(true)).unwrap();
        assert!(tasks.iter().flat_map(|t| &t.models).all(|m| m.system || !m.installed), "macOS owns Apple Speech");
        assert_eq!(active(&conn, tmp.path(), 15), [Some("parakeet-tdt-v3".into()), Some("who".into())]);
    }

    #[test]
    fn macos_14_falls_back_from_desert_ant_and_cant_pick_it() {
        let (tmp, conn) = db();
        assert_eq!(active(&conn, tmp.path(), 14), [Some("parakeet-tdt-v3".into()), Some("sortformer".into())]);
        assert!(select(&conn, "diar", "who", 14, &english(true)).unwrap_err().contains("macOS 15"));
    }

    /// Parakeet and the ONNX Runtime it needs.
    fn installed_parakeet(models: &Path) {
        for m in [find("parakeet-tdt-v3").unwrap(), runtime()] {
            let dir = models.join(&m.id);
            std::fs::create_dir_all(&dir).unwrap();
            for f in m.files {
                std::fs::File::create(dir.join(&f.path)).unwrap().set_len(f.size).unwrap(); // sparse
            }
        }
    }

    #[test]
    fn apple_speech_is_active_until_parakeet_is_downloaded() {
        let (tmp, conn) = db();
        let models = tmp.path().join("models");
        let stt = |sys: &System, macos| list(&conn, &models, macos, sys).unwrap().remove(0);
        assert_eq!(stt(&english(false), 26).active.as_deref(), Some("apple-speech"), "not installed still wins over a download");
        assert_eq!(stt(&System::default(), 26).active.as_deref(), Some("parakeet-tdt-v3"), "unsupported language");
        assert_eq!(stt(&english(true), 15).active.as_deref(), Some("parakeet-tdt-v3"), "macOS 15");
        installed_parakeet(&models);
        assert_eq!(stt(&english(true), 26).active.as_deref(), Some("parakeet-tdt-v3"));
        select(&conn, "stt", "apple-speech", 26, &english(true)).unwrap();
        assert_eq!(stt(&english(true), 26).active.as_deref(), Some("apple-speech"), "a pick sticks");
    }

    #[test]
    fn apple_speech_row_says_its_language_or_what_it_needs() {
        let (tmp, conn) = db();
        let row = |sys: &System, macos| {
            list(&conn, &tmp.path().join("models"), macos, sys).unwrap().remove(0).models.into_iter().find(|m| m.system).unwrap()
        };
        let r = row(&english(true), 26);
        assert_eq!((r.desc.as_str(), r.installed, r.needs), ("Built into macOS. Listens in English (India).", true, None));
        assert_eq!(row(&english(true), 15).needs.as_deref(), Some("Needs macOS 26"));
        assert_eq!(row(&System::default(), 26).needs.as_deref(), Some("Needs a language Apple supports"));
        assert!(!row(&english(false), 26).installed);
        assert_eq!(
            select(&conn, "stt", "apple-speech", 26, &System::default()).unwrap_err(),
            "Apple Speech: Needs a language Apple supports."
        );
    }

    #[test]
    fn deleting_the_chosen_model_hands_over_to_the_default() {
        let (tmp, conn) = db();
        let models = tmp.path().join("models");
        installed_parakeet(&models);
        let stt = |conn: &Connection| list(conn, &models, 26, &english(true)).unwrap().remove(0).active;
        select(&conn, "stt", "whisper-large-v3-turbo", 26, &english(true)).unwrap();
        forget(&conn, "parakeet-tdt-v3").unwrap();
        assert_eq!(stt(&conn).as_deref(), Some("whisper-large-v3-turbo"), "only the deleted model's pick goes");
        forget(&conn, "whisper-large-v3-turbo").unwrap();
        assert_eq!(stt(&conn).as_deref(), Some("parakeet-tdt-v3"));
    }

    #[test]
    fn choice_persists_across_reopen() {
        let (tmp, conn) = db();
        select(&conn, "stt", "whisper-large-v3-turbo", 15, &english(true)).unwrap();
        select(&conn, "stt", "voz", 15, &english(true)).unwrap();
        drop(conn);
        let conn = crate::storage::open(tmp.path()).unwrap();
        assert_eq!(active(&conn, tmp.path(), 15)[0].as_deref(), Some("voz"));
        assert!(select(&conn, "stt", "nope", 15, &english(true)).is_err());
    }

    #[test]
    fn installed_when_every_file_is_present_at_full_size() {
        let (tmp, conn) = db();
        let dir = tmp.path().join("models/whisper-large-v3-turbo");
        std::fs::create_dir_all(&dir).unwrap();
        let f = std::fs::File::create(dir.join("ggml-large-v3-turbo.bin")).unwrap();
        let installed = |conn: &Connection| {
            list(conn, &tmp.path().join("models"), 15, &english(true)).unwrap()[0].models.iter().find(|m| m.id == "whisper-large-v3-turbo").unwrap().installed
        };
        assert!(!installed(&conn), "partial file");
        f.set_len(1624555275).unwrap(); // sparse, no real disk use
        assert!(installed(&conn));
    }

    #[test]
    fn an_unknown_or_non_numeric_unload_time_reads_5_minutes() {
        let (_tmp, conn) = db();
        conn.execute("INSERT INTO setting VALUES ('stt_unload_secs', '7')", []).unwrap();
        assert_eq!(unload_after(&conn), Ok(300));
        conn.execute("UPDATE setting SET value = 'never' WHERE key = 'stt_unload_secs'", []).unwrap();
        assert_eq!(unload_after(&conn), Ok(300));
    }

    // Advanced: custom models.

    fn locked(conn: Connection) -> std::sync::Mutex<Connection> {
        std::sync::Mutex::new(conn)
    }

    fn no_hf(_: &str) -> Result<String, String> {
        panic!("a path never asks Hugging Face")
    }

    #[test]
    fn a_path_that_loads_is_added_in_place_and_one_that_doesnt_isnt() {
        let (tmp, conn) = db();
        let db = locked(conn);
        let models = tmp.path().join("models");
        let bad = tmp.path().join("bad.bin");
        std::fs::write(&bad, b"not a model").unwrap();
        let real = |m: &Model| crate::runtime::check(m, &models);
        let err = add(&db, &models, "whisper", bad.to_str().unwrap(), &no_hf, &real).unwrap_err();
        assert!(err.starts_with("Couldn't load bad.bin"), "{err}");
        assert!(customs(&db.lock().unwrap()).unwrap().is_empty(), "a model that won't load isn't added");
        assert_eq!(add(&db, &models, "whisper", "/nothing/here.bin", &no_hf, &real).unwrap_err(), "There's nothing at /nothing/here.bin.");

        // Loading works as with the runtime tests' fakes: the real load needs a downloaded model (slow test below).
        let mine = tmp.path().join("ggml-mine.bin");
        std::fs::write(&mine, b"weights").unwrap();
        let m = add(&db, &models, "whisper", mine.to_str().unwrap(), &no_hf, &|_| Ok(())).unwrap();
        assert_eq!((m.id.as_str(), m.name.as_str(), m.vendor.as_str(), m.installed), ("custom-ggml-mine-bin", "ggml-mine.bin", "On this Mac", true));
        assert!(!models.join(&m.id).exists(), "used in place, not copied");
        let conn = db.lock().unwrap();
        for task in list(&conn, &models, 26, &english(true)).unwrap().into_iter().filter(|t| t.id != "diar") {
            let row = task.models.iter().find(|x| x.id == m.id).expect("a row in both speech-to-text lists");
            assert!(row.installed && row.needs.is_none());
        }
        assert!(list(&conn, &models, 26, &english(true)).unwrap()[1].models.iter().all(|x| x.id != m.id), "not a diarization model");
        select(&conn, "stt", &m.id, 26, &english(true)).unwrap();
        assert_eq!(active(&conn, &models, 26)[0].as_deref(), Some(m.id.as_str()));
        drop(conn);
        assert_eq!(add(&db, &models, "whisper", mine.to_str().unwrap(), &no_hf, &|_| Ok(())).unwrap_err(), "ggml-mine.bin is already in the list.");
    }

    #[test]
    #[ignore = "slow: needs a downloaded Whisper model"]
    fn a_downloaded_whisper_model_added_by_path_loads() {
        let (tmp, conn) = db();
        let models = PathBuf::from(std::env::var_os("HOME").unwrap()).join("Library/Application Support/Myna/models");
        let file = models.join("whisper-small/ggml-small.bin");
        let m = add(&locked(conn), tmp.path(), "whisper", file.to_str().unwrap(), &no_hf, &|m| crate::runtime::check(m, tmp.path())).unwrap();
        assert!(m.installed);
    }

    #[test]
    fn an_onnx_model_on_this_mac_waits_for_onnx_runtime() {
        let (tmp, conn) = db();
        let models = tmp.path().join("models");
        let dir = tmp.path().join("my-parakeet");
        std::fs::create_dir_all(&dir).unwrap();
        let m = add(&locked(conn), &models, "parakeet-tdt", dir.to_str().unwrap(), &no_hf, &|_| panic!("can't load before the runtime is there")).unwrap();
        assert!(!m.installed && m.runtime);
    }

    #[test]
    fn hugging_face_repos_parse_from_ids_and_links() {
        let p = |s: &str| parse_hf(s).map(|(r, v, a)| format!("{r}@{v}:{a}"));
        assert_eq!(p("ggerganov/whisper.cpp").as_deref(), Some("ggerganov/whisper.cpp@main:"));
        assert_eq!(p(" https://huggingface.co/ggerganov/whisper.cpp/ ").as_deref(), Some("ggerganov/whisper.cpp@main:"));
        assert_eq!(p("https://huggingface.co/ggerganov/whisper.cpp/blob/main/ggml-small.bin?download=true").as_deref(), Some("ggerganov/whisper.cpp@main:ggml-small.bin"));
        assert_eq!(p("huggingface.co/altunenes/parakeet-rs/tree/664cc742/parakeet-unified-en-0.6b").as_deref(), Some("altunenes/parakeet-rs@664cc742:parakeet-unified-en-0.6b"));
        assert_eq!(p("https://huggingface.co/o/r/resolve/v1/a/b.onnx").as_deref(), Some("o/r@v1:a/b.onnx"));
        for bad in ["", "whisper", "o/r/x", "https://example.com/o/r", "o/r/tree/main/../../etc", "../r", "o/r r", "https://huggingface.co/o/r/tree"] {
            assert_eq!(p(bad), None, "{bad}");
        }
    }

    fn listing(files: &[(&str, u64, bool)]) -> Vec<HfFile> {
        files.iter().map(|&(p, size, lfs)| HfFile { rfilename: p.into(), size, lfs: lfs.then(|| HfLfs { sha256: format!("sha-{p}") }) }).collect()
    }

    fn picked(engine: &str, list: &[HfFile], at: &str) -> Result<Vec<String>, String> {
        let label = ENGINES.iter().find(|e| e.0 == engine).unwrap().2;
        pick(engine, label, "o/r", list, at).map(|f| f.into_iter().map(|f| format!("{} {} {}", f.path, f.size, f.sha256)).collect())
    }

    #[test]
    fn each_runtime_picks_the_files_it_needs() {
        // istupakov/parakeet-tdt-0.6b-v3-onnx: the int8 export, as in the catalog, not the 2.4 GB one.
        let tdt = listing(&[("README.md", 1, false), ("config.json", 97, false), ("decoder_joint-model.int8.onnx", 18, true), ("decoder_joint-model.onnx", 72, true),
            ("encoder-model.int8.onnx", 652, true), ("encoder-model.onnx", 41, true), ("encoder-model.onnx.data", 2435, true), ("nemo128.onnx", 1, true), ("vocab.txt", 93, false)]);
        assert_eq!(picked("parakeet-tdt", &tdt, "").unwrap(), ["encoder-model.int8.onnx 652 sha-encoder-model.int8.onnx", "decoder_joint-model.int8.onnx 18 sha-decoder_joint-model.int8.onnx", "vocab.txt 93 "]);
        assert_eq!(picked("parakeet-unified", &tdt, "").unwrap_err(), "There's no Parakeet Unified tokenizer.model in o/r.");
        // A folder in the repo, with weights in a .data file beside the encoder; a link to one of its files means its folder.
        let unified = listing(&[("sortformer.onnx", 4, true), ("u/encoder.onnx", 4, true), ("u/encoder.onnx.data", 2478, true), ("u/decoder_joint.onnx", 35, true), ("u/tokenizer.model", 1, true)]);
        let want = ["u/encoder.onnx", "u/encoder.onnx.data", "u/decoder_joint.onnx", "u/tokenizer.model"];
        for at in ["u", "u/encoder.onnx"] {
            assert_eq!(picked("parakeet-unified", &unified, at).unwrap().iter().map(|f| f.split(' ').next().unwrap()).collect::<Vec<_>>(), want);
        }
        assert_eq!(picked("parakeet-unified", &unified, "").unwrap_err(), "There's no Parakeet Unified encoder in o/r.", "not from below the folder");
        // One file: the only one there, or the one linked.
        assert_eq!(picked("sortformer", &unified, "").unwrap(), ["sortformer.onnx 4 sha-sortformer.onnx"]);
        let whisper = listing(&[("ggml-small.bin", 487, true), ("ggml-medium.bin", 1500, true), ("README.md", 1, false), ("../../evil.bin", 1, true), ("/abs.bin", 1, true)]);
        assert_eq!(picked("whisper", &whisper, "").unwrap_err(), "o/r has 2 Whisper models. Paste the link to the one you want.");
        assert_eq!(picked("whisper", &whisper, "ggml-small.bin").unwrap(), ["ggml-small.bin 487 sha-ggml-small.bin"]);
        assert_eq!(picked("whisper", &whisper, "README.md").unwrap_err(), "README.md isn't a Whisper model (a .bin file).");
        assert_eq!(picked("whisper", &listing(&[("README.md", 1, false)]), "").unwrap_err(), "There's no Whisper model (a .bin file) in o/r.");
        assert_eq!(picked("whisper", &listing(&[("ok.bin", 1, true), ("../x.bin", 1, true)]), "").unwrap().len(), 1, "a path that climbs out is never picked");
    }

    const REPO: &str = r#"{"sha": "5359861c739e955e79d9a303bcbc70fb988958b1", "siblings": [
        {"rfilename": "README.md", "size": 10}, {"rfilename": "ggml-small.bin", "size": 487, "lfs": {"sha256": "abc", "size": 487}}]}"#;

    #[test]
    fn a_hugging_face_repo_is_added_to_download_pinned_to_its_commit() {
        let (tmp, conn) = db();
        let db = locked(conn);
        let models = tmp.path().join("models");
        let asked = std::cell::RefCell::new(String::new());
        let get = |url: &str| {
            *asked.borrow_mut() = url.into();
            Ok(REPO.to_string())
        };
        let m = add(&db, &models, "whisper", "https://huggingface.co/ggerganov/whisper.cpp", &get, &|_| panic!("loaded once it's downloaded")).unwrap();
        assert_eq!(*asked.borrow(), "https://huggingface.co/api/models/ggerganov/whisper.cpp/revision/main?blobs=true");
        assert_eq!((m.id.as_str(), m.name.as_str(), m.repo.as_str(), m.revision.as_str()), ("custom-ggml-small-bin", "ggml-small.bin", "ggerganov/whisper.cpp", "5359861c739e955e79d9a303bcbc70fb988958b1"));
        assert_eq!((m.size, m.installed, m.path.as_str(), m.desc.as_str()), (487, false, "", "Whisper · ggerganov/whisper.cpp"));
        assert_eq!(add(&db, &models, "whisper", "ggerganov/whisper.cpp", &get, &|_| Ok(())).unwrap_err(), "ggml-small.bin is already in the list.");
        let err = add(&db, &models, "whisper", "o/private", &|_| Err("http status: 401".into()), &|_| Ok(())).unwrap_err();
        assert_eq!(err, "Couldn't find o/private on Hugging Face (http status: 401). Check the name, and that the repo is public.");
        assert!(add(&db, &models, "whisper", "not a repo", &get, &|_| Ok(())).unwrap_err().starts_with("Paste a path"));
        assert_eq!(add(&db, &models, "llama", "o/r", &get, &|_| Ok(())).unwrap_err(), "Unknown runtime llama");
    }

    #[test]
    fn removing_a_custom_model_never_deletes_the_users_own_files() {
        let (tmp, conn) = db();
        let db = locked(conn);
        let models = tmp.path().join("models");
        let mine = tmp.path().join("mine");
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::write(mine.join("ggml-mine.bin"), b"weights").unwrap();
        let local = add(&db, &models, "whisper", mine.join("ggml-mine.bin").to_str().unwrap(), &no_hf, &|_| Ok(())).unwrap();
        let hf = add(&db, &models, "whisper", "o/r", &|_| Ok(REPO.to_string()), &|_| Ok(())).unwrap();
        std::fs::create_dir_all(models.join(&hf.id)).unwrap(); // as a finished download leaves it
        let conn = db.lock().unwrap();
        let all = everything(&conn).unwrap();
        for m in [&local, &hf] {
            crate::download::remove(m, &models, &all, &[]).unwrap();
            remove_custom(&conn, &m.id).unwrap();
        }
        assert!(mine.join("ggml-mine.bin").exists(), "the user's file stays");
        assert!(!models.join(&hf.id).exists(), "a download goes");
        assert!(customs(&conn).unwrap().is_empty());
    }

    #[test]
    fn custom_models_persist_and_a_newer_myna_s_entries_survive() {
        let (tmp, conn) = db();
        // A newer Myna added one for a runtime this one lacks, with a field this one doesn't know.
        crate::storage::write_text(&conn, CUSTOM, r#"[{"id": "custom-future", "name": "F", "size": 0, "files": [], "engine": "warp", "extra": 1}]"#).unwrap();
        let db = locked(conn);
        let models = tmp.path().join("models");
        let m = add(&db, &models, "whisper", "o/r", &|_| Ok(REPO.to_string()), &|_| Ok(())).unwrap();
        drop(db);
        let conn = crate::storage::open(tmp.path()).unwrap();
        assert_eq!(customs(&conn).unwrap().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), [m.id.as_str()], "listed after a reopen; the unknown runtime isn't");
        assert_eq!(get(&conn, &m.id).unwrap().files.len(), 1);
        remove_custom(&conn, &m.id).unwrap();
        let raw = crate::storage::read_text(&conn, CUSTOM).unwrap().unwrap();
        assert!(raw.contains(r#""engine":"warp""#) && raw.contains(r#""extra":1"#), "{raw}");
    }

    #[test]
    fn a_custom_model_that_didnt_load_says_why_and_cant_be_picked() {
        let (tmp, conn) = db();
        let db = locked(conn);
        let models = tmp.path().join("models");
        let m = add(&db, &models, "whisper", "o/r", &|_| Ok(REPO.to_string()), &|_| Ok(())).unwrap();
        let conn = db.lock().unwrap();
        mark_broken(&conn, &m.id, "Couldn't load ggml-small.bin: bad magic").unwrap();
        let row = list(&conn, &models, 26, &english(true)).unwrap().remove(0).models.into_iter().find(|x| x.id == m.id).unwrap();
        assert_eq!((row.needs.as_deref(), row.desc.as_str()), (Some("Won't load"), "Couldn't load ggml-small.bin: bad magic"));
        assert_eq!(select(&conn, "stt", &m.id, 26, &english(true)).unwrap_err(), "ggml-small.bin: Won't load.");
    }

    #[test]
    fn an_onnx_model_added_before_onnx_runtime_cant_be_picked_until_it_loads() {
        let (tmp, conn) = db();
        let db = locked(conn);
        let models = tmp.path().join("models");
        let mine = tmp.path().join("diar.onnx");
        std::fs::write(&mine, b"weights").unwrap();
        let m = add(&db, &models, "sortformer", mine.to_str().unwrap(), &no_hf, &|_| panic!("no ONNX Runtime to load it on")).unwrap();
        assert!(!m.installed && !m.checked);
        // Another model's download installs ONNX Runtime.
        let rt = runtime();
        for f in &rt.files {
            let at = models.join(&rt.id).join(&f.path);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::File::create(at).unwrap().set_len(f.size).unwrap();
        }
        let conn = db.lock().unwrap();
        let row = |conn: &Connection| list(conn, &models, 26, &english(true)).unwrap().remove(1).models.into_iter().find(|x| x.id == m.id).unwrap();
        assert!(!row(&conn).installed, "never loaded: it offers Download, which checks it");
        assert_eq!(select(&conn, "diar", &m.id, 26, &english(true)).unwrap_err(), "diar.onnx hasn't loaded yet. Download it to check it.");
        mark_checked(&conn, &m.id).unwrap();
        assert!(row(&conn).installed);
        select(&conn, "diar", &m.id, 26, &english(true)).unwrap();
    }

    #[test]
    fn a_custom_model_that_loads_after_failing_can_be_picked_again() {
        let (tmp, conn) = db();
        let db = locked(conn);
        let models = tmp.path().join("models");
        let m = add(&db, &models, "whisper", "o/r", &|_| Ok(REPO.to_string()), &|_| Ok(())).unwrap();
        std::fs::create_dir_all(models.join(&m.id)).unwrap();
        std::fs::File::create(models.join(&m.id).join("ggml-small.bin")).unwrap().set_len(487).unwrap();
        let conn = db.lock().unwrap();
        mark_broken(&conn, &m.id, "Couldn't load ggml-small.bin: bad magic").unwrap();
        mark_broken(&conn, &m.id, "Couldn't load ggml-small.bin: still bad").unwrap();
        mark_checked(&conn, &m.id).unwrap(); // Download again, and this time it loads
        let row = list(&conn, &models, 26, &english(true)).unwrap().remove(0).models.into_iter().find(|x| x.id == m.id).unwrap();
        assert_eq!((row.needs, row.desc.as_str(), row.installed), (None, "Whisper · o/r", true));
        select(&conn, "stt", &m.id, 26, &english(true)).unwrap();
    }
}
