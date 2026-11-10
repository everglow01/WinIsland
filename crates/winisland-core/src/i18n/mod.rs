use std::collections::{BTreeMap, HashMap};
use std::sync::{LazyLock, OnceLock};

use parking_lot::RwLock;

#[derive(Clone, Debug)]
pub struct Language {
    pub code: String,
    pub name: String,
}

pub struct I18n {
    pub current_lang: String,
    translations: HashMap<String, String>,
    pub available_languages: Vec<Language>,
    plugin_translation_bundles: BTreeMap<u64, PluginTranslationBundle>,
}

struct PluginTranslationBundle {
    language: String,
    translations: HashMap<String, String>,
}

type EmbeddedLang = (&'static str, &'static str);

static I18N: LazyLock<RwLock<I18n>> = LazyLock::new(|| RwLock::new(I18n::new()));

fn embedded_langs() -> &'static [EmbeddedLang] {
    &[
        (
            "en_us",
            include_str!("../../../../resources/in_app/lang/en_us.lang"),
        ),
        (
            "zh_cn",
            include_str!("../../../../resources/in_app/lang/zh_cn.lang"),
        ),
    ]
}

fn lang_dir() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("resources")
        .join("in_app")
        .join("lang")
}

fn parse_lang_name(content: &str) -> Option<String> {
    content
        .lines()
        .take_while(|line| line.starts_with('!'))
        .find_map(|line| line.strip_prefix("!lang_name="))
        .map(|name| name.trim().to_owned())
}

fn parse_translations(content: &str) -> HashMap<String, String> {
    content
        .lines()
        .filter_map(|line| {
            (!line.starts_with('!'))
                .then(|| line.split_once('='))
                .flatten()
        })
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

fn format_args(template: String, args: &[&str]) -> String {
    args.iter()
        .enumerate()
        .fold(template, |result, (index, value)| {
            result
                .replacen("{}", value, 1)
                .replace(&format!("{{{index}}}"), value)
        })
}

impl I18n {
    fn new() -> Self {
        let available_languages = embedded_langs()
            .iter()
            .map(|(code, embedded)| {
                let content = Self::load_file_content(code, embedded);
                Language {
                    code: (*code).to_string(),
                    name: parse_lang_name(&content).unwrap_or_else(|| (*code).to_string()),
                }
            })
            .collect();
        let mut i18n = I18n {
            current_lang: "en_us".to_string(),
            translations: HashMap::new(),
            available_languages,
            plugin_translation_bundles: BTreeMap::new(),
        };
        i18n.load("en_us");
        i18n
    }

    fn load_file_content(lang: &str, embedded: &str) -> String {
        std::fs::read_to_string(lang_dir().join(format!("{lang}.lang")))
            .unwrap_or_else(|_| embedded.to_string())
    }

    pub fn load(&mut self, lang: &str) {
        let lang = if self
            .available_languages
            .iter()
            .any(|language| language.code == lang)
        {
            lang
        } else if lang.starts_with("zh") {
            "zh_cn"
        } else {
            "en_us"
        };
        if let Some((_, embedded)) = embedded_langs().iter().find(|(code, _)| *code == lang) {
            let content = Self::load_file_content(lang, embedded);
            self.current_lang = lang.to_string();
            self.translations = embedded_langs()
                .iter()
                .find(|(code, _)| *code == "en_us")
                .map(|(_, content)| parse_translations(content))
                .unwrap_or_default();
            self.translations.extend(parse_translations(&content));
        }
    }

    pub fn get(&self, key: &str) -> String {
        if let Some(value) = self
            .plugin_translation_bundles
            .values()
            .rev()
            .filter(|bundle| bundle.language == self.current_lang)
            .find_map(|bundle| bundle.translations.get(key))
        {
            return value.clone();
        }
        self.translations
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    pub fn register_plugin_translation_bundle(
        &mut self,
        id: u64,
        language: String,
        pairs: Vec<(String, String)>,
    ) {
        self.plugin_translation_bundles.insert(
            id,
            PluginTranslationBundle {
                language,
                translations: pairs.into_iter().collect(),
            },
        );
    }

    pub fn release_plugin_translation_bundle(&mut self, id: u64) {
        self.plugin_translation_bundles.remove(&id);
    }
}

static SYSTEM_LOCALE_PROVIDER: OnceLock<fn() -> String> = OnceLock::new();

/// Registers the platform locale lookup used when the configured language is `auto`.
///
/// The provider is read on every `init_i18n("auto")` call; until it is registered, `auto`
/// resolves to `en_us`.
pub fn set_system_locale_provider(provider: fn() -> String) {
    let _ = SYSTEM_LOCALE_PROVIDER.set(provider);
}

pub fn init_i18n(config_lang: &str) {
    let target_lang = if config_lang == "auto" {
        get_system_lang()
    } else {
        config_lang.to_string()
    };
    I18N.write().load(&target_lang);
}

pub fn set_lang(lang: &str) {
    I18N.write().load(lang);
}

pub fn current_lang() -> String {
    I18N.read().current_lang.clone()
}

pub fn tr(key: &str) -> String {
    I18N.read().get(key)
}

pub fn tr_args(key: &str, args: &[&str]) -> String {
    let template = I18N.read().get(key);
    format_args(template, args)
}

pub fn available_langs() -> Vec<Language> {
    I18N.read().available_languages.clone()
}

pub fn register_plugin_translation_bundle(
    id: u64,
    language: String,
    pairs: Vec<(String, String)>,
) -> Result<(), &'static str> {
    I18N.write()
        .register_plugin_translation_bundle(id, language, pairs);
    Ok(())
}

pub fn release_plugin_translation_bundle(id: u64) -> Result<(), &'static str> {
    I18N.write().release_plugin_translation_bundle(id);
    Ok(())
}

fn get_system_lang() -> String {
    let Some(provider) = SYSTEM_LOCALE_PROVIDER.get() else {
        return "en_us".to_string();
    };
    let lower = provider().to_lowercase();
    if lower.starts_with("zh") {
        return "zh_cn".to_string();
    }
    "en_us".to_string()
}
