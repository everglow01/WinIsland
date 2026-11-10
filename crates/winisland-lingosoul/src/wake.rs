//! Turns plain-text wake words into the keyword spotter's token format:
//! Chinese characters become toned pinyin split into initial + final, English
//! words become CMU phones from the model's `en.phone` lexicon.
//!
//! One phrase can produce several token lines with the same label, so neutral
//! tones (芝麻 = zhī ma) and other readings of polyphones (银行 = yín háng) still
//! match what people actually say.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use pinyin::{ToPinyin, ToPinyinMulti};

const INITIALS: [&str; 23] = [
    "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x", "r",
    "z", "c", "s", "y", "w",
];
const MAX_VARIANTS: usize = 16;

struct Vocabulary {
    tokens: HashSet<String>,
    lexicon: HashMap<String, Vec<String>>,
}

impl Vocabulary {
    fn load(model_dir: &Path) -> Self {
        let tokens = std::fs::read_to_string(model_dir.join("tokens.txt"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
            .collect();
        let mut lexicon = HashMap::new();
        for line in std::fs::read_to_string(model_dir.join("en.phone"))
            .unwrap_or_default()
            .lines()
        {
            let mut parts = line.split_whitespace();
            if let Some(word) = parts.next() {
                lexicon
                    .entry(word.to_owned())
                    .or_insert_with(|| parts.map(str::to_owned).collect());
            }
        }
        Self { tokens, lexicon }
    }
}

/// One spoken unit: a Chinese syllable with its candidate readings, or an English word's phones.
enum Unit {
    Syllable(Vec<String>),
    Phones(Vec<String>),
}

fn split_syllable(syllable: &str) -> Vec<String> {
    for initial in INITIALS {
        if let Some(rest) = syllable.strip_prefix(initial)
            && !rest.is_empty()
        {
            return vec![initial.to_owned(), rest.to_owned()];
        }
    }
    vec![syllable.to_owned()]
}

fn toneless(syllable: &str) -> String {
    syllable
        .chars()
        .map(|c| match c {
            'ā' | 'á' | 'ǎ' | 'à' => 'a',
            'ē' | 'é' | 'ě' | 'è' => 'e',
            'ī' | 'í' | 'ǐ' | 'ì' => 'i',
            'ō' | 'ó' | 'ǒ' | 'ò' => 'o',
            'ū' | 'ú' | 'ǔ' | 'ù' => 'u',
            'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' => 'ü',
            other => other,
        })
        .collect()
}

fn units(phrase: &str, vocab: &Vocabulary) -> Result<Vec<Unit>, String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<Unit>| -> Result<(), String> {
        if word.is_empty() {
            return Ok(());
        }
        let phones = vocab
            .lexicon
            .get(&word.to_uppercase())
            .ok_or_else(|| format!("The word '{word}' is not in the pronunciation dictionary"))?;
        out.push(Unit::Phones(phones.clone()));
        word.clear();
        Ok(())
    };
    for c in phrase.chars() {
        if c.is_ascii_alphabetic() || c == '\'' {
            word.push(c);
            continue;
        }
        flush(&mut word, &mut out)?;
        if let Some(main) = c.to_pinyin() {
            let mut readings = vec![main.with_tone().to_owned()];
            if let Some(multi) = c.to_pinyin_multi() {
                for reading in multi {
                    let reading = reading.with_tone().to_owned();
                    if !readings.contains(&reading) {
                        readings.push(reading);
                    }
                }
            }
            out.push(Unit::Syllable(readings));
        } else if !c.is_whitespace() && !c.is_ascii_punctuation() && !"，。！？、".contains(c)
        {
            return Err(format!("Unsupported wake-word character: '{c}'"));
        }
    }
    flush(&mut word, &mut out)?;
    if out.is_empty() {
        return Err("The wake word is empty".into());
    }
    Ok(out)
}

/// Reading choices per syllable: the main reading, then one change at a time
/// (a later syllable in neutral tone, or another polyphone reading).
fn variants(units: &[Unit]) -> Vec<Vec<String>> {
    let base: Vec<Option<String>> = units
        .iter()
        .map(|unit| match unit {
            Unit::Syllable(readings) => Some(readings[0].clone()),
            Unit::Phones(_) => None,
        })
        .collect();
    let mut choices = vec![base.clone()];
    let mut paired = base.clone();
    let mut position = 0;
    for (i, unit) in units.iter().enumerate() {
        if let Unit::Syllable(readings) = unit {
            if position % 2 == 1 {
                paired[i] = Some(toneless(&readings[0]));
            }
            position += 1;
        }
    }
    choices.push(paired);
    let mut seen_syllable = false;
    for (i, unit) in units.iter().enumerate() {
        let Unit::Syllable(readings) = unit else {
            continue;
        };
        if seen_syllable {
            let mut neutral = base.clone();
            neutral[i] = Some(toneless(&readings[0]));
            choices.push(neutral);
        }
        seen_syllable = true;
        for other in readings.iter().skip(1) {
            let mut alt = base.clone();
            alt[i] = Some(other.clone());
            choices.push(alt);
        }
    }
    let mut lines = Vec::new();
    for choice in choices {
        let mut tokens = Vec::new();
        for (unit, reading) in units.iter().zip(&choice) {
            match (unit, reading) {
                (Unit::Phones(phones), _) => tokens.extend(phones.iter().cloned()),
                (Unit::Syllable(_), Some(syllable)) => tokens.extend(split_syllable(syllable)),
                (Unit::Syllable(_), None) => {}
            }
        }
        if !lines.contains(&tokens) {
            lines.push(tokens);
        }
        if lines.len() >= MAX_VARIANTS {
            break;
        }
    }
    lines
}

/// Returns the keyword spotter input plus one warning per wake word that couldn't be used.
pub fn keywords(words: &[String], model_dir: &Path) -> (String, Vec<String>) {
    let vocab = Vocabulary::load(model_dir);
    let mut lines = String::new();
    let mut warnings = Vec::new();
    for phrase in words.iter().map(|w| w.trim()).filter(|w| !w.is_empty()) {
        let label: String = phrase
            .chars()
            .map(|c| if c.is_whitespace() { '_' } else { c })
            .collect();
        match units(phrase, &vocab) {
            Ok(units) => {
                let usable: Vec<_> = variants(&units)
                    .into_iter()
                    .filter(|tokens| tokens.iter().all(|t| vocab.tokens.contains(t)))
                    .collect();
                if usable.is_empty() {
                    warnings.push(format!(
                        "Wake word '{phrase}' has no pronunciation in the model vocabulary"
                    ));
                }
                for tokens in usable {
                    lines.push_str(&format!("{} @{label}\n", tokens.join(" ")));
                }
            }
            Err(reason) => warnings.push(format!("Wake word '{phrase}' is unavailable: {reason}")),
        }
    }
    (lines, warnings)
}
