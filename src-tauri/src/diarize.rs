//! Speaker separation for a meeting's system track: who spoke when. Backends are Desert Ant's Who
//! (through the Swift bridge, see who.rs) and NVIDIA Sortformer (parakeet-rs). The mic track is always
//! Me, so only the system track comes through here.
use crate::models;
use std::path::Path;

pub const WHO: &str = "who";
pub const SORTFORMER: &str = "sortformer-v3";

/// One stretch of one speaker. The backend numbers its speakers; turns of different speakers may overlap.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Turn {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: u32,
}

/// A speaker-separation backend. Takes mono 16 kHz samples of any length.
pub trait Diarizer: Send {
    fn diarize(&mut self, audio: &[f32]) -> Result<Vec<Turn>, String>;
}

pub struct Sortformer(parakeet_rs::sortformer::Sortformer);

impl Sortformer {
    pub fn load(file: &Path) -> Result<Self, String> {
        parakeet_rs::sortformer::Sortformer::with_config(file, Some(crate::dictation::ort_config()?), Default::default())
            .map(Self)
            .map_err(|e| e.to_string())
    }
}

impl Diarizer for Sortformer {
    // shortcut: the whole track goes through in one pass (fine for a few hours of speech); chunk it with
    // `feed` if memory ever matters.
    fn diarize(&mut self, audio: &[f32]) -> Result<Vec<Turn>, String> {
        let segments = self.0.diarize(audio.to_vec(), crate::audio::RATE, 1).map_err(|e| e.to_string())?;
        let ms = |samples: u64| samples * 1000 / u64::from(crate::audio::RATE);
        Ok(segments.into_iter().map(|s| Turn { start_ms: ms(s.start), end_ms: ms(s.end), speaker: s.speaker_id as u32 }).collect())
    }
}

/// Why a curated diarization model can't run on this Mac or in this build.
pub fn unavailable(id: &str) -> Option<String> {
    (id == WHO).then(crate::who::unavailable).flatten()
}

/// The model diarization uses: the user's pick, else the first curated one that can run here.
pub fn pick(chosen: Option<&str>, unavailable: impl Fn(&str) -> bool) -> &'static str {
    let all = || models::MODELS.iter().filter(|m| m.task == "diarization");
    all().find(|m| chosen == Some(m.id)).or_else(|| all().find(|m| !unavailable(m.id))).unwrap_or_else(|| all().next().unwrap()).id
}

/// Loads the backend for a downloaded model, or says why it can't be used.
pub fn load(id: &str, root: &Path) -> Result<Box<dyn Diarizer>, String> {
    let m = models::MODELS.iter().find(|m| m.id == id && m.task == "diarization").ok_or("Unknown speaker model.")?;
    if models::status(root, m) != models::Status::Installed {
        return Err(format!("The {} model isn't downloaded. Get it under Settings → Models.", m.name));
    }
    let dir = root.join(id);
    Ok(match id {
        WHO => Box::new(crate::who::Who::load(&dir)?),
        SORTFORMER => Box::new(Sortformer::load(&dir.join(m.files[0].name))?),
        _ => return Err("Unknown speaker model.".into()),
    })
}

/// A cluster per distinct speaker in `turns`, numbered from 1 in order of first appearance in `lines`
/// (start, end in ms). Each line goes to the speaker talking for most of it, or, when no turn overlaps it,
/// to the nearest turn. `None` only when there are no turns at all.
// shortcut: a line that spans two speakers goes whole to the one who talks more of it; split by word
// timing if that reads too coarsely.
pub fn assign(lines: &[(u64, u64)], turns: &[Turn]) -> Vec<Option<u32>> {
    let mut numbered: Vec<u32> = vec![];
    lines
        .iter()
        .map(|&(start, end)| {
            let speaker = speaker_at(turns, start, end)?;
            let n = numbered.iter().position(|&s| s == speaker).unwrap_or_else(|| {
                numbered.push(speaker);
                numbered.len() - 1
            });
            Some(n as u32 + 1)
        })
        .collect()
}

fn speaker_at(turns: &[Turn], start: u64, end: u64) -> Option<u32> {
    let mut overlap: Vec<(u32, u64)> = vec![];
    for t in turns {
        let shared = t.end_ms.min(end).saturating_sub(t.start_ms.max(start));
        match overlap.iter_mut().find(|(s, _)| *s == t.speaker) {
            Some(o) => o.1 += shared,
            None => overlap.push((t.speaker, shared)),
        }
    }
    // Most overlap wins; a tie goes to the lower speaker number.
    let best = overlap.iter().filter(|o| o.1 > 0).min_by_key(|&&(s, shared)| (std::cmp::Reverse(shared), s));
    best.map(|o| o.0).or_else(|| {
        let gap = |t: &Turn| if t.end_ms <= start { start - t.end_ms } else { t.start_ms.saturating_sub(end) };
        turns.iter().min_by_key(|t| (gap(t), t.speaker)).map(|t| t.speaker)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models;
    use std::process::Command;

    fn turn(start_ms: u64, end_ms: u64, speaker: u32) -> Turn {
        Turn { start_ms, end_ms, speaker }
    }

    #[test]
    fn each_line_goes_to_the_speaker_with_the_most_overlap_and_clusters_count_from_one_by_first_appearance() {
        // (name, turns, lines, clusters)
        let table: [(&str, Vec<Turn>, Vec<(u64, u64)>, Vec<Option<u32>>); 8] = [
            ("two speakers", vec![turn(0, 4000, 0), turn(4000, 9000, 1)], vec![(500, 3500), (4500, 8000)], vec![Some(1), Some(2)]),
            ("backend numbering is renumbered by who speaks first in the lines", vec![turn(0, 4000, 3), turn(4000, 9000, 0)], vec![(500, 3500), (4500, 8000), (9000, 9500)], vec![Some(1), Some(2), Some(2)]),
            ("a line spanning two speakers goes to the one with more of it", vec![turn(0, 3000, 0), turn(3000, 9000, 1)], vec![(2000, 8000), (0, 1000)], vec![Some(1), Some(2)]),
            ("a tie goes to the lower speaker number", vec![turn(0, 2000, 1), turn(2000, 4000, 0)], vec![(1000, 3000), (0, 500)], vec![Some(1), Some(2)]),
            ("overlapping turns: the longer one wins", vec![turn(0, 6000, 0), turn(1000, 2000, 1), turn(7000, 9000, 1)], vec![(0, 5000), (6500, 9000)], vec![Some(1), Some(2)]),
            ("a speaker's turns add up", vec![turn(0, 1000, 0), turn(1000, 3000, 1), turn(3000, 4000, 0), turn(4000, 5000, 0)], vec![(0, 5000), (1500, 2500)], vec![Some(1), Some(2)]),
            ("no overlap: nearest turn", vec![turn(0, 1000, 0), turn(9000, 10_000, 1)], vec![(2000, 3000), (7000, 8000)], vec![Some(1), Some(2)]),
            ("no turns", vec![], vec![(0, 1000)], vec![None]),
        ];
        for (name, turns, lines, want) in table {
            assert_eq!(assign(&lines, &turns), want, "{name}");
        }
    }

    #[test]
    fn the_default_model_is_the_pick_else_the_first_that_runs_here() {
        // (name, chosen, who unavailable, model)
        let table = [
            ("default", None, false, WHO),
            ("who can't run, so the fallback", None, true, SORTFORMER),
            ("the user's pick stands", Some(SORTFORMER), false, SORTFORMER),
            ("an unknown pick is ignored", Some("nope"), false, WHO),
            ("a pick stands even if it can't run; loading says why", Some(WHO), true, WHO),
        ];
        for (name, chosen, who_off, want) in table {
            assert_eq!(pick(chosen, |id| who_off && id == WHO), want, "{name}");
        }
    }

    #[test]
    fn a_model_that_is_not_downloaded_says_so_instead_of_loading() {
        let root = std::env::temp_dir().join(format!("myna-diar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for id in [WHO, SORTFORMER] {
            let err = load(id, &root).err().expect("nothing is installed");
            assert!(err.contains("isn't downloaded"), "{err}");
        }
        assert!(load("parakeet-tdt-0.6b-v3", &root).is_err());
    }

    /// `say` speaking each line in its voice as 16 kHz mono samples, a second of quiet after each, and where each line sits (ms).
    pub fn say(root: &Path, lines: &[(&str, &str)]) -> (Vec<f32>, Vec<(u64, u64)>) {
        let (mut all, mut at) = (vec![], vec![]);
        for (i, (voice, text)) in lines.iter().enumerate() {
            let (aiff, wav) = (root.join(format!("{i}.aiff")), root.join(format!("{i}.wav")));
            assert!(Command::new("say").args(["-v", voice, "-o"]).arg(&aiff).arg(text).status().unwrap().success());
            assert!(Command::new("afconvert").args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"]).arg(&aiff).arg(&wav).status().unwrap().success());
            let start = all.len() as u64 * 1000 / 16_000;
            all.extend(hound::WavReader::open(&wav).unwrap().samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0));
            at.push((start, all.len() as u64 * 1000 / 16_000));
            all.extend(std::iter::repeat(0.0).take(16_000));
        }
        (all, at)
    }

    pub fn install(root: &Path, id: &str) {
        let m = models::MODELS.iter().find(|m| m.id == id).unwrap();
        if models::status(root, m) != models::Status::Installed {
            models::install(root, m.id, m.files, models::http_open, &models::Job::default()).unwrap();
        }
        assert!(models::verify(root, m), "{id} failed its checksums");
    }

    // Real models on two `say` voices, the first talking twice. Run by hand, in release:
    // `cargo test --release -- --ignored real_diarizers --nocapture`. Needs macOS 15+ (Who) and `say`;
    // downloads Who (84 MB) and Sortformer (492 MB) once into $TMPDIR/myna-diarize.
    #[test]
    #[ignore]
    fn real_diarizers_tell_two_say_voices_apart() {
        let root = std::env::temp_dir().join("myna-diarize");
        std::fs::create_dir_all(&root).unwrap();
        let (samples, lines) = say(&root, &[
            ("Samantha", "Good morning everyone. Let's start with the roadmap review for the next quarter, and then go through the open risks."),
            ("Daniel", "Thanks for having me. I want to flag that the launch date depends on the security review, which is not finished yet."),
            ("Samantha", "Understood. Then let's move the launch by one week, and I will tell the customers about the new date this afternoon."),
        ]);
        for id in [WHO, SORTFORMER] {
            install(&root, id);
            let turns = load(id, &root).unwrap().diarize(&samples).unwrap();
            println!("{id}: {turns:?}");
            assert_eq!(assign(&lines, &turns), [Some(1), Some(2), Some(1)], "{id} should hear voice A, voice B, voice A again");
        }
    }
}
