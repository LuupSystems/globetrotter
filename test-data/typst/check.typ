// Checks that the generated module imports and that Typst reads its strings back exactly.
#import "translations.typ": arguments, engine, languages, translations, version
#assert.eq(version, "1")
#assert.eq(engine, "handlebars")
#assert.eq(languages, ("en", "de"))
#assert.eq(translations.keys(), languages)
#for lang in languages {
  assert.eq(translations.at(lang).keys(), arguments.keys())
}
#assert.eq(arguments.at("plain"), (:))
#assert.eq(arguments.at("dash-key"), ("arg-name": "string"))
#assert.eq(arguments.at("cart.items"), (count: "number"))
#let en = translations.at("en")
#assert.eq(en.at("plain"), "Hello \"world\" \\\\ café 😀")
#assert.eq(en.at("greeting"), "Hello {{name}}!")
#assert.eq(en.at("choice"), "{{#if enabled}}Ready{{else}}Waiting{{/if}}")
#assert.eq(en.at("escaped"), "line one\n\"quotes\" \\ tab\tcontrol\u{1} {{{name}}}")
The translation module is well-formed.
