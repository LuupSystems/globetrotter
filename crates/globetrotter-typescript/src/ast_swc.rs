use globetrotter_model as model;
use std::sync::Arc;
use swc_core::{
    base::{Compiler, PrintArgs},
    common::DUMMY_SP,
    ecma::ast,
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Codegen(anyhow::Error),
}

pub trait IntoAST<T> {
    fn into_ast(self) -> T;
}

impl IntoAST<ast::TsType> for model::ArgumentType {
    fn into_ast(self) -> ast::TsType {
        match self {
            Self::String => ast::TsType::TsKeywordType(ast::TsKeywordType {
                span: DUMMY_SP,
                kind: ast::TsKeywordTypeKind::TsStringKeyword,
            }),
            Self::Number => ast::TsType::TsKeywordType(ast::TsKeywordType {
                span: DUMMY_SP,
                kind: ast::TsKeywordTypeKind::TsNumberKeyword,
            }),
            Self::Iso8601DateTimeString => ast::TsType::TsKeywordType(ast::TsKeywordType {
                span: DUMMY_SP,
                kind: ast::TsKeywordTypeKind::TsStringKeyword,
            }),
            Self::Any => ast::TsType::TsKeywordType(ast::TsKeywordType {
                span: DUMMY_SP,
                kind: ast::TsKeywordTypeKind::TsAnyKeyword,
            }),
        }
    }
}

fn emit_code(compiler: &Compiler, program: &ast::Program) -> Result<String, anyhow::Error> {
    let compiled = compiler.print(
        program,
        PrintArgs {
            preamble: &crate::preamble(),
            ..PrintArgs::default()
        },
    )?;
    Ok(compiled.code)
}

fn type_annotation_for_translation(translation: &model::Translation) -> ast::TsType {
    if !translation.is_template() {
        // "translation.key": string;
        return model::ArgumentType::String.into_ast();
    }

    // "translation.key": (values: { readonly "member.one": string; }) => string;
    let members = translation
        .arguments
        .iter()
        .map(|(name, argument)| {
            let key = ast::Expr::Lit(ast::Lit::Str(ast::Str {
                span: DUMMY_SP,
                value: name.to_string().into(),
                raw: None,
            }));
            ast::TsTypeElement::TsPropertySignature(ast::TsPropertySignature {
                span: DUMMY_SP,
                readonly: true,
                key: Box::new(key),
                computed: false,
                optional: false,
                type_ann: Some(Box::new(ast::TsTypeAnn {
                    span: DUMMY_SP,
                    type_ann: Box::new(argument.into_ast()),
                })),
            })
        })
        .collect::<Vec<_>>();

    let values = ast::TsType::TsTypeLit(ast::TsTypeLit {
        span: DUMMY_SP,
        members,
    });

    let values_param = ast::TsFnParam::Ident(ast::BindingIdent {
        id: ast::Ident::new_no_ctxt("values".into(), DUMMY_SP),
        type_ann: Some(Box::new(ast::TsTypeAnn {
            span: DUMMY_SP,
            type_ann: Box::new(values),
        })),
    });

    let return_type = ast::TsTypeAnn {
        span: DUMMY_SP,
        type_ann: Box::new(model::ArgumentType::String.into_ast()),
    };

    ast::TsType::TsFnOrConstructorType(ast::TsFnOrConstructorType::TsFnType(ast::TsFnType {
        span: DUMMY_SP,
        params: vec![values_param],
        type_params: None,
        type_ann: Box::new(return_type),
    }))
}

fn type_members(
    translations: &model::Translations,
) -> impl Iterator<Item = ast::TsTypeElement> + use<'_> {
    translations.0.iter().map(|(key, translation)| {
        let type_annotation = type_annotation_for_translation(translation);
        let key = ast::Expr::Lit(ast::Lit::Str(ast::Str {
            span: DUMMY_SP,
            value: key.to_string().into(),
            raw: None,
        }));
        ast::TsTypeElement::TsPropertySignature(ast::TsPropertySignature {
            span: DUMMY_SP,
            readonly: true,
            key: Box::new(key),
            computed: false,
            optional: false,
            type_ann: Some(Box::new(ast::TsTypeAnn {
                span: DUMMY_SP,
                type_ann: Box::new(type_annotation),
            })),
        })
    })
}

pub fn generate_translations_type_export(
    translations: &model::Translations,
) -> Result<String, Error> {
    let members: Vec<_> = type_members(translations).collect();

    let program = ast::Program::Module(ast::Module {
        span: DUMMY_SP,
        body: vec![ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(
            ast::ExportDecl {
                span: DUMMY_SP,
                decl: ast::Decl::TsTypeAlias(Box::new(ast::TsTypeAliasDecl {
                    span: DUMMY_SP,
                    declare: false,
                    id: ast::Ident::new_no_ctxt("Translations".into(), DUMMY_SP),
                    type_params: None,
                    type_ann: Box::new(ast::TsType::TsTypeLit(ast::TsTypeLit {
                        span: DUMMY_SP,
                        members,
                    })),
                })),
            },
        ))],
        shebang: None,
    });

    let cm: swc_core::common::sync::Lrc<swc_core::common::SourceMap> = Default::default();
    let compiler = Compiler::new(cm);

    emit_code(&compiler, &program).map_err(Error::Codegen)
}

#[cfg(test)]
mod tests {
    use color_eyre::eyre;
    use globetrotter_model::{self as model, diagnostics::Spanned};
    use indoc::indoc;
    use similar_asserts::assert_eq as sim_assert_eq;
    use std::sync::Arc;
    use swc_core::{
        base::{Compiler, PrintArgs},
        common::DUMMY_SP,
        ecma::ast,
    };

    trait IntoEyre {
        fn into_eyre(self) -> eyre::Report;
    }

    impl IntoEyre for anyhow::Error {
        fn into_eyre(self) -> eyre::Report {
            eyre::eyre!(Box::new(self))
        }
    }

    fn parse(
        compiler: &Compiler,
        fm: Arc<swc_core::common::SourceFile>,
    ) -> eyre::Result<ast::Program> {
        let emitter_writer = swc_core::common::errors::EmitterWriter::new(
            Box::new(std::io::stderr()),
            Some(compiler.cm.clone()),
            true,
            false,
        );
        let handler =
            swc_core::common::errors::Handler::with_emitter(true, false, Box::new(emitter_writer));

        let program = compiler
            .parse_js(
                fm,
                &handler,
                swc_core::ecma::ast::EsVersion::Es2022,
                swc_core::ecma::parser::Syntax::Typescript(Default::default()),
                swc_core::base::config::IsModule::Unknown,
                Some(compiler.comments()),
            )
            .map_err(IntoEyre::into_eyre)?;
        Ok(program)
    }

    #[test]
    fn generate_type() -> eyre::Result<()> {
        crate::tests::init();

        let translations = model::Translations(
            [
                (
                    Spanned::dummy("test.one".to_string()),
                    model::Translation {
                        language: [(
                            model::Language::En,
                            Spanned::dummy("test.one in en".to_string()),
                        )]
                        .into_iter()
                        .collect(),
                        arguments: [].into_iter().collect(),
                        file_id: 0,
                    },
                ),
                (
                    Spanned::dummy("test.two".to_string()),
                    model::Translation {
                        language: [(
                            model::Language::En,
                            Spanned::dummy("test.two in en".to_string()),
                        )]
                        .into_iter()
                        .collect(),
                        arguments: [
                            ("arg-one".to_string(), model::ArgumentType::String),
                            ("ArgTwo".to_string(), model::ArgumentType::Number),
                            ("Arg_Three".to_string(), model::ArgumentType::Any),
                        ]
                        .into_iter()
                        .collect(),
                        file_id: 0,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        );
        let expected = indoc::indoc! {r#"
            export type Translations = {
                readonly "test.one": string;
                readonly "test.two": (values: {
                    readonly "arg-one": string;
                    readonly "ArgTwo": number;
                    readonly "Arg_Three": any;
                }) => string;
            };
        "# };
        let expected = format!("{}{}", crate::preamble(), expected);
        let generated = super::generate_translations_type_export(&translations)?;
        sim_assert_eq!(generated, expected);
        Ok(())
    }

    #[test]
    fn parse_reference_interface() -> eyre::Result<()> {
        crate::tests::init();

        let source_code = indoc! {r#"
            export type Translations = {
                test: string;
                "other.value": (values: {
                    a: number;
                    b: string;
                }) => string;
            };
        "#};

        let cm: swc_core::common::sync::Lrc<swc_core::common::SourceMap> = Default::default();
        let fm = cm.new_source_file(
            swc_core::common::FileName::Custom("./test.ts".to_string()).into(),
            source_code.to_string(),
        );
        let compiler = Compiler::new(cm);

        let program = parse(&compiler, fm)?;
        dbg!(&program);

        let code = super::emit_code(&compiler, &program).map_err(IntoEyre::into_eyre)?;
        println!("{}", code);

        sim_assert_eq!(format!("{}{}", crate::preamble(), source_code), code);
        Ok(())
    }
}
