use globetrotter_model as model;
use oxc_allocator::Allocator;
use oxc_parser::{ParseOptions, Parser};

// prettier using oxc: https://github.com/oxc-project/oxc/blob/main/crates/oxc_prettier/examples/prettier.rs

pub fn export_translations_type(
    translations: &model::Translations,
    language: model::Language,
) -> eyre::Result<String> {
    let allocator = Allocator::default();
    let source_type = oxc_span::SourceType::from_path("./test.ts")?;
    let source_text = "const ref: number = 3;";
    let ret = Parser::new(&allocator, source_text, source_type)
        .with_options(ParseOptions {
            preserve_parens: false,
            ..ParseOptions::default()
        })
        .parse();
    println!("AST:");
    println!("{}", serde_json::to_string_pretty(&ret.program)?);
    unimplemented!("oxc not currently supported");
    Ok("".to_string())
}
