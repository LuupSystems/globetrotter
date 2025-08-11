use color_eyre::eyre;
use globetrotter::{
    Language,
    config::{
        self,
        v1::{Config, ConfigFile, Input, JsonOutputConfig, Outputs},
    },
    model::TemplateEngine,
};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let manifest_dir: PathBuf = std::env::var("CARGO_MANIFEST_DIR")?.into();
    let out_dir: PathBuf = std::env::var("OUT_DIR")?.into();

    // Use translations.toml as input
    let translations_file = manifest_dir.join("translations.toml");
    println!("cargo:rerun-if-changed={}", translations_file.display());

    let input = Input::new(translations_file.to_string_lossy());

    // Output translations as json and emit rust bindings
    let outputs = Outputs::new()
        .with_json([JsonOutputConfig::new(
            out_dir.join("translations_{{language}}.json"),
        )])
        .with_rust(config::rust::OutputConfig::new([
            out_dir.join("translations.rs")
        ]));

    let config = Config::new("translations")
        .with_strict(true)
        .with_languages([Language::De, Language::En, Language::Fr])
        .with_check_templates(true)
        .with_template_engine(TemplateEngine::Handlebars)
        .with_input(input)
        .with_outputs(outputs);

    let configs = vec![ConfigFile {
        file_id: None,
        config_dir: None,
        config,
    }];

    // Generate
    let diagnostic_printer = globetrotter::diagnostics::Printer::default();
    let executor = globetrotter::Executor::new(&configs, diagnostic_printer);
    executor.execute(configs).await?;
    Ok(())
}
