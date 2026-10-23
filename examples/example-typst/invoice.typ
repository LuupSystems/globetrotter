// Renders an invoice in the language passed with `--input lang=de`.
#import "../../runtime/typst/globetrotter/lib.typ": translator
#import "generated/translations.typ" as catalog

#let lang = sys.inputs.at("lang", default: "en")
#let t = translator(catalog, lang, helpers: (
  // Only English templates call this helper; German wording needs no plural suffix.
  pluralize: (word, count) => if count == 1 { word } else { word + "s" },
))
#let status = "open"

#set text(lang: lang)

= #t("invoice.title")

#t("invoice.greeting", name: "Ada Lovelace")

#t("invoice.items", count: 3)

#t("invoice.total", amount: 42.5, paid: false)

// A concatenated key keeps every `invoice.status.*` key in use under the default dynamic policy.
#t("invoice.status." + status)
