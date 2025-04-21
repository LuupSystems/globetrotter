use crate::options::{self, InputFormat};
use clap::builder::PossibleValuesParser;
use color_eyre::eyre::{self, WrapErr};
use futures::AsyncWriteExt;
use globetrotter::error::IoError;
use globetrotter::model::{
    Arguments, Language, LanguageTranslations, Translation, Translations, diagnostics::Spanned,
};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(thiserror::Error, Debug)]
#[error("custom key style {0:?}")]
pub struct CustomStyle(crate::options::KeyStyle);

impl TryInto<convert_case::Case<'static>> for crate::options::KeyStyle {
    type Error = CustomStyle;
    fn try_into(self) -> Result<convert_case::Case<'static>, Self::Error> {
        use convert_case::Case;
        match self {
            custom @ (Self::Dotted | Self::UpperDotted) => Err(CustomStyle(custom)),
            Self::Upper => Ok(Case::Upper),
            Self::Lower => Ok(Case::Lower),
            Self::Title => Ok(Case::Title),
            Self::Pascal => Ok(Case::Pascal),
            Self::Toggle => Ok(Case::Toggle),
            Self::Camel => Ok(Case::Camel),
            Self::UpperCamel => Ok(Case::UpperCamel),
            Self::Snake => Ok(Case::Snake),
            Self::UpperSnake => Ok(Case::UpperSnake),
            Self::ScreamingSnake => Ok(Case::UpperSnake),
            Self::Kebab | Self::Hyphens => Ok(Case::Kebab),
            Self::UpperKebab | Self::UpperHyphens => Ok(Case::UpperKebab),
            Self::Cobol => Ok(Case::Cobol),
            Self::Train => Ok(Case::Train),
            Self::Flat => Ok(Case::Flat),
            Self::UpperFlat => Ok(Case::UpperFlat),
            Self::Alternating => Ok(Case::Alternating),
        }
    }
}

trait ValueKind {
    fn kind(&self) -> &'static str;
}

impl ValueKind for Value {
    fn kind(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Object(_) => "map",
            Value::Array(_) => "array",
            Value::String(_) => "string",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
        }
    }
}

pub fn build_translation(value: &Value, languages: &HashSet<&Language>) -> Translation {
    let language = languages.iter().filter_map(|lang| {
        let lang_key = lang.code();
        let map = value.as_object()?;
        let translation = map.get(lang_key)?;
        let translation = translation.as_str()?;
        Some((**lang, Spanned::dummy(translation.to_string())))
    });

    Translation {
        arguments: Arguments::default(),
        language: language.collect(),
        file_id: 0,
    }
}

pub fn convert_json_mapping(
    (key, value): (String, serde_json::Value),
    languages: &HashSet<&Language>,
) -> (Spanned<String>, Translation) {
    (Spanned::dummy(key), build_translation(&value, languages))
}

pub fn convert_json_entry(
    value: serde_json::Value,
    languages: &HashSet<&Language>,
) -> eyre::Result<(Spanned<String>, Translation)> {
    if let Some((key, value)) = value.as_object().and_then(|object| {
        let key = object.get("key").and_then(|key| key.as_str())?;
        let value = object.get("value")?;
        Some((key, value))
    }) {
        return Ok((
            Spanned::dummy(key.to_string()),
            build_translation(value, languages),
        ));
    }
    if let Some((key, value)) = value.as_array().and_then(|array| {
        let key = array.first().and_then(|key| key.as_str())?;
        let value = array.get(1)?;
        Some((key, value))
    }) {
        return Ok((
            Spanned::dummy(key.to_string()),
            build_translation(value, languages),
        ));
    }
    if let Some((key, value)) = value.as_array().and_then(|array| {
        let key = array.first().and_then(|key| key.as_str())?;
        let value = array.first()?;
        Some((key, value))
    }) {
        return Ok((
            Spanned::dummy(key.to_string()),
            build_translation(value, languages),
        ));
    }
    Err(eyre::eyre!("expected key value pair, got {}", value.kind()))
}

pub fn parse_json_input(input: &str, languages: &HashSet<&Language>) -> eyre::Result<Translations> {
    let mut translations: serde_json::Value = serde_json::from_str(input)?;

    match translations.take() {
        serde_json::Value::Object(mapping) => Ok(mapping
            .into_iter()
            .map(|entry| convert_json_mapping(entry, languages))
            .collect()),
        serde_json::Value::Array(array) => array
            .into_iter()
            .map(|entry| convert_json_entry(entry, languages))
            .collect::<Result<Translations, _>>(),
        other => Err(eyre::eyre!(
            "expected a map of translation keys or a flat list of translations, got {}",
            other.kind()
        )),
    }
}

pub mod fmt {
    use toml_edit::visit_mut::{VisitMut, visit_table_like_kv_mut};
    use toml_edit::{InlineTable, Item, KeyMut, Table, Value};

    #[derive(Debug)]
    struct DocumentFormatter {
        depth: usize,
    }

    impl VisitMut for DocumentFormatter {
        fn visit_table_like_kv_mut(&mut self, mut key: KeyMut<'_>, node: &mut Item) {
            let old_depth = self.depth;

            self.depth += 1;

            if old_depth == 0 {
                if let Item::Value(Value::InlineTable(inline_table)) = node {
                    let inline_table = std::mem::replace(inline_table, InlineTable::new());
                    let mut table = inline_table.into_table();
                    key.fmt();
                    *node = Item::Table(table);
                }
            }

            // recurse further into the document tree.
            visit_table_like_kv_mut(self, key, node);

            // restore the old state
            self.depth = old_depth;
        }
    }

    pub fn format_document(doc: &mut toml_edit::Document) {
        let mut visitor = DocumentFormatter { depth: 0 };
        visitor.visit_document_mut(doc);
    }
}

fn change_case(value: &mut String, case: options::KeyStyle) {
    use convert_case::{Case, Casing};

    match case.try_into().ok() {
        None => match case {
            options::KeyStyle::Dotted => {
                *value = value.to_case(Case::Kebab).replace('-', ".");
            }
            options::KeyStyle::UpperDotted => {
                *value = value.to_case(Case::UpperKebab).replace('-', ".");
            }
            other => tracing::warn!("key style {other:?} not implemented"),
        },
        Some(case) => {
            *value = value.to_case(case);
        }
    }
}

fn to_document(
    value: &(impl serde::ser::Serialize + ?Sized),
) -> Result<toml_edit::DocumentMut, toml_edit::ser::Error> {
    let mut doc: toml_edit::DocumentMut = toml_edit::ser::to_document(value)?;
    fmt::format_document(&mut doc);
    Ok(doc)
}

pub fn postprocess(
    mut translations: Translations,
    options: &options::ConvertOptions,
) -> eyre::Result<Translations> {
    for (k, t) in translations.iter() {
        if t.is_empty() {
            tracing::warn!(key = k.display().to_string(), "empty translation");
        }
    }
    if options.sort != Some(false) {
        translations.sort();
    }

    if let Some(case) = options.style {
        translations = translations
            .into_iter()
            .map(|(mut k, v)| {
                change_case(&mut k.inner, case);
                (k, v)
            })
            .collect();
    }

    if let Some(ref prefix) = options.prefix {
        translations = translations
            .into_iter()
            .map(|(mut k, v)| {
                match options.style {
                    Some(case) => {
                        k.inner = format!("{} {}", prefix, k.inner);
                        change_case(&mut k.inner, case);
                    }
                    None => {
                        k.inner = format!("{}{}", prefix, k.inner);
                    }
                }
                (k, v)
            })
            .collect();
    }
    Ok(translations)
}

pub async fn convert_file(
    path: &Path,
    options: Arc<options::ConvertOptions>,
) -> eyre::Result<Translations> {
    let input = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| IoError::new(path, err))?;

    let input_format = options
        .input_format
        .or_else(|| {
            path.extension()
                // .and_then(|path| path.extension())
                .and_then(|ext| ext.to_str())
                .and_then(InputFormat::from_ext)
        })
        .ok_or_else(|| eyre::eyre!("cannot detect file type for {}", path.display()))?;
    // .ok_or_else(|| match file_path {
    //     Some(path) => eyre::eyre!("cannot detect file type for {}", path.display())
    //     None => eyre::eyre!("cannot detect file type"
    // }

    // let file_name = input_path.file_name().and_then(|name| name.to_str());
    let translations =
        tokio::task::spawn_blocking(move || convert_str(&input, input_format, options)).await??;
    Ok(translations)
}

pub fn convert_str(
    value: &str,
    format: InputFormat,
    options: Arc<options::ConvertOptions>,
) -> eyre::Result<Translations> {
    let languages: HashSet<_> = options.languages.iter().collect();
    let translations = match format {
        InputFormat::Json => parse_json_input(value, &languages),
        other => Err(eyre::eyre!(
            "cannot convert input file with extension {other:?}"
        )),
    }?;
    postprocess(translations, &options)
}

pub async fn convert(mut options: options::ConvertOptions) -> eyre::Result<()> {
    let options = Arc::new(options);
    let translations = convert_file(&options.input_path, Arc::clone(&options)).await?;
    dbg!(translations.len());

    if let Some(ref output_path) = options.output_path {
        use tokio::io::AsyncWriteExt;

        let doc: toml_edit::DocumentMut = to_document(&translations)?;

        dbg!(&output_path);
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(output_path)
            .await?;

        let mut writer = tokio::io::BufWriter::new(file);
        writer.write_all(doc.to_string().as_bytes()).await?;
        writer.flush().await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        Spanned,
        model::{ArgumentType, Arguments, Language, Translation, Translations},
    };
    use color_eyre::eyre;
    use similar_asserts::assert_eq as sim_assert_eq;
    use std::sync::Arc;

    #[test]
    fn test_convert_json_object() -> eyre::Result<()> {
        crate::tests::init();
        let json = serde_json::json!({
          "climateChange": {
            "code": "E1",
            "id": null,
            "parent": null,
            "en": "Climate change",
            "de": "Klimawandel",
            "fr": "Changement climatique"
          },
          "climateChangeAdaptation": {
            "code": "E1:1",
            "id": "TopicId.E11",
            "parent": "climateChange",
            "en": "Climate change adaptation",
            "de": "Anpassung an den Klimawandel",
            "fr": "Adaptation au changement climatique"
          }
        });
        let json_str = serde_json::to_string_pretty(&json)?;
        println!("{json_str}");
        let options = Arc::new(crate::options::ConvertOptions {
            languages: vec![Language::De, Language::En, Language::Fr],
            ..crate::options::ConvertOptions::default()
        });
        let have = super::convert_str(&json_str, super::InputFormat::Json, options)?;
        dbg!(&have);

        let want: Translations = [
            (
                Spanned::dummy("climateChange".to_string()),
                Translation {
                    arguments: Arguments::default(),
                    language: [
                        (Language::De, Spanned::dummy("Klimawandel".to_string())),
                        (Language::En, Spanned::dummy("Climate change".to_string())),
                        (
                            Language::Fr,
                            Spanned::dummy("Changement climatique".to_string()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                    file_id: 0,
                },
            ),
            (
                Spanned::dummy("climateChangeAdaptation".to_string()),
                Translation {
                    arguments: Arguments::default(),
                    language: [
                        (
                            Language::De,
                            Spanned::dummy("Anpassung an den Klimawandel".to_string()),
                        ),
                        (
                            Language::En,
                            Spanned::dummy("Climate change adaptation".to_string()),
                        ),
                        (
                            Language::Fr,
                            Spanned::dummy("Adaptation au changement climatique".to_string()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                    file_id: 0,
                },
            ),
        ]
        .into_iter()
        .collect();

        sim_assert_eq!(have, want);

        Ok(())
    }

    #[test]
    fn test_convert_json_array() -> eyre::Result<()> {
        crate::tests::init();
        // spellcheck:ignore-block
        let json = serde_json::json!([
          [
            "climateChangeAdaptationDescription",
            {
              "de": "Dieser Aspekt behandelt den Einfluss extremer Wetterereignisse, die durch den Klimawandel verursacht werden und somit das Geschäft beinflussen.",
              "en": "This aspect deals with the impact of extreme weather events caused by climate change and thus affecting the business.",
              "fr": "Cet aspect traite de l'impact des événements météorologiques extrêmes causés par le changement climatique et affectant ainsi l'entreprise."
            }
          ],
          [
            "climateChangeAdaptationDescriptionLaw",
            {
              "de": "Das Gesetz verlangt, die Anfälligkeit für klimatische Risiken zu bewerten und offenzulegen.",
              "en": "The law requires to assess and disclose the vulnerability to climatic risks.",
              "fr": "La loi exige d'évaluer et de divulguer la vulnérabilité aux risques climatiques."
            }
          ],
        ]);
        let json_str = serde_json::to_string_pretty(&json)?;
        println!("{json_str}\n");
        let options = Arc::new(crate::options::ConvertOptions {
            languages: vec![Language::De, Language::En],
            ..crate::options::ConvertOptions::default()
        });
        let have = super::convert_str(&json_str, super::InputFormat::Json, options)?;
        dbg!(&have);

        let want: Translations = [
            (
                Spanned::dummy("climateChangeAdaptationDescription".to_string()),
                Translation {
                    arguments: Arguments::default(),
                    language: [
                        (
                            Language::De,
                            Spanned::dummy("Dieser Aspekt behandelt den Einfluss extremer Wetterereignisse, die durch den Klimawandel verursacht werden und somit das Geschäft beinflussen.".to_string()),
                        ),
                        (
                            Language::En,
                            Spanned::dummy("This aspect deals with the impact of extreme weather events caused by climate change and thus affecting the business.".to_string()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                    file_id: 0,
                },
            ),
            (
                Spanned::dummy("climateChangeAdaptationDescriptionLaw".to_string()),
                Translation {
                    arguments: Arguments::default(),
                    language: [
                        (
                            Language::De,
                            Spanned::dummy("Das Gesetz verlangt, die Anfälligkeit für klimatische Risiken zu bewerten und offenzulegen.".to_string()),
                        ),
                        (
                            Language::En,
                            Spanned::dummy("The law requires to assess and disclose the vulnerability to climatic risks.".to_string()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                    file_id: 0,
                },
            ),
        ]
        .into_iter()
        .collect();

        sim_assert_eq!(have, want);
        Ok(())
    }

    #[test]
    fn test_into_toml_document() -> eyre::Result<()> {
        crate::tests::init();

        let mut translations: Translations = [
            (
                Spanned::dummy("key.two".to_string()),
                Translation {
                    arguments: [
                        ("arg.3 ".to_string(), ArgumentType::Number),
                        ("arg.2   ".to_string(), ArgumentType::String),
                    ]
                    .into_iter()
                    .collect(),
                    language: [
                        (
                            Language::En,
                            Spanned::dummy("english translation".to_string()),
                        ),
                        (
                            Language::De,
                            Spanned::dummy("german translation".to_string()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                    file_id: 1,
                },
            ),
            (
                Spanned::dummy("key.one".to_string()),
                Translation {
                    arguments: [
                        ("arg2".to_string(), ArgumentType::Number),
                        ("arg1".to_string(), ArgumentType::String),
                    ]
                    .into_iter()
                    .collect(),
                    language: [
                        (
                            Language::En,
                            Spanned::dummy("english translation".to_string()),
                        ),
                        (
                            Language::De,
                            Spanned::dummy("german translation".to_string()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                    file_id: 0,
                },
            ),
        ]
        .into_iter()
        .collect();
        translations.sort();

        let doc: toml_edit::DocumentMut = super::to_document(&translations)?;
        println!("\n=======================\n{}", doc);

        sim_assert_eq!(
            doc.to_string(),
            indoc::indoc! {
                r#"
                ["key.one"]
                de = "german translation"
                en = "english translation"
                arguments = { arg1 = "string", arg2 = "number" }

                ["key.two"]
                de = "german translation"
                en = "english translation"
                arguments = { "arg.2   " = "string", "arg.3 " = "number" }
                "#
            }
        );
        Ok(())
    }
}
