//! Local offline translation using Opus-MT models via rust-bert
//!
//! Provides CJK-to-English translation without sending data to external servers.
//! Uses Helsinki-NLP's Opus-MT (MarianMT) models, which are automatically
//! downloaded from HuggingFace Hub on first use (~300 MB per language pair).
//!
//! # Requirements
//! - Build with `--features local-translate`
//! - Install libtorch (see README for setup instructions)
//!
//! # Supported language pairs
//! - Chinese → English  (`Helsinki-NLP/opus-mt-zh-en`)
//! - Japanese → English (`Helsinki-NLP/opus-mt-ja-en`)
//! - Korean → English   (`Helsinki-NLP/opus-mt-ko-en`)

use crate::detector::Language;
use crate::error::{Error, Result};

#[cfg(feature = "local-translate")]
mod implementation {
    use super::*;
    use rust_bert::pipelines::translation::{
        Language as RBLanguage, TranslationModel, TranslationModelBuilder,
    };
    use std::sync::{Mutex, OnceLock};

    /// Global translation model (loaded once, reused across calls).
    /// Uses Mutex because TranslationModel may not be Sync.
    static LOCAL_MODEL: OnceLock<Mutex<TranslationModel>> = OnceLock::new();

    /// Initialize and return the shared translation model.
    fn get_model() -> Result<&'static Mutex<TranslationModel>> {
        LOCAL_MODEL.get_or_try_init(|| {
            let model = TranslationModelBuilder::new()
                .with_source_languages(vec![
                    RBLanguage::ChineseMandarin,
                    RBLanguage::Japanese,
                    RBLanguage::Korean,
                ])
                .with_target_languages(vec![RBLanguage::English])
                .create_model()
                .map_err(|e| Error::Translation {
                    message: format!("Failed to load Opus-MT model: {e}"),
                })?;
            Ok(Mutex::new(model))
        })
    }

    /// Map internal Language enum to rust-bert's Language enum.
    fn to_rb_language(lang: Language) -> Option<RBLanguage> {
        match lang {
            Language::Chinese => Some(RBLanguage::ChineseMandarin),
            Language::Japanese => Some(RBLanguage::Japanese),
            Language::Korean => Some(RBLanguage::Korean),
            _ => None,
        }
    }

    /// Translate text locally using Opus-MT models.
    ///
    /// This function is blocking (ML inference). When called from async context,
    /// wrap with `tokio::task::spawn_blocking`.
    pub fn translate_local_blocking(text: &str, source_lang: Language) -> Result<String> {
        let model_mutex = get_model()?;
        let model = model_mutex.lock().map_err(|e| Error::Translation {
            message: format!("Model lock poisoned: {e}"),
        })?;

        let source = to_rb_language(source_lang);

        let output = model
            .translate(&[text], source, RBLanguage::English)
            .map_err(|e| Error::Translation {
                message: format!("Local translation failed: {e}"),
            })?;

        output.into_iter().next().ok_or_else(|| Error::Translation {
            message: "Empty translation result".into(),
        })
    }

    /// Translate text locally (async wrapper using spawn_blocking).
    pub async fn translate_local(text: &str, source_lang: Language) -> Result<String> {
        let text = text.to_string();
        let result = tokio::task::spawn_blocking(move || {
            translate_local_blocking(&text, source_lang)
        })
        .await
        .map_err(|e| Error::Translation {
            message: format!("Local translation task failed: {e}"),
        })??;

        Ok(result)
    }
}

#[cfg(not(feature = "local-translate"))]
mod implementation {
    use super::*;

    /// Stub: local translation is not available without the `local-translate` feature.
    pub async fn translate_local(_text: &str, _source_lang: Language) -> Result<String> {
        Err(Error::Translation {
            message: "Local translation requires the 'local-translate' feature. \
                      Build with: cargo build --features local-translate"
                .into(),
        })
    }
}

pub use implementation::translate_local;
