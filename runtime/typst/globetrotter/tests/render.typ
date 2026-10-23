// Checks every supported construct against the generated fixture module.
#import "../lib.typ": translator
#import "translations.typ" as catalog

#let helpers = (
  pluralize: (word, count) => if count == 1 { word } else { word + "s" },
  join: (..values) => values.pos().map(repr).join(","),
  today: () => "2026-01-01",
)
#let t = translator(catalog, "en", helpers: helpers)
#let de = translator(catalog, "de", helpers: helpers)

// Literals
#assert.eq(t("literal"), "Plain {{text}} stays verbatim")
#assert.eq(de("literal"), "Schlicht {{text}} bleibt wörtlich")

// Values, triple-stash, and segment literals render plain text without escaping
#assert.eq(t("greeting", name: "Ada"), "Hello Ada!")
#assert.eq(de("greeting", name: "Ada"), "Hallo Ada!")
#assert.eq(t("triple", name: "<b>&"), "<b>& & <b>&")
#assert.eq(t("segment", arg-name: "x"), "x = x")

// Conditions
#assert.eq(t("choice", enabled: true), "on")
#assert.eq(t("choice", enabled: false), "off")
#assert.eq(t("unless", enabled: false), "disabled")
#assert.eq(t("unless", enabled: true), "")
#assert.eq(t("nested", outer: true, inner: false), "A")
#assert.eq(t("nested", outer: true, inner: true), "B")
#assert.eq(t("nested", outer: false, inner: false), "C")

// Handlebars truthiness: none, false, zero, empty strings, and empty arrays are false
#for value in (none, false, 0, 0.0, "", ()) {
  assert.eq(t("truthy", value: value), "no", message: repr(value))
}
#for value in (true, 1, -0.5, "0", (0,), (:)) {
  assert.eq(t("truthy", value: value), "yes", message: repr(value))
}

// Comments, including long comments that contain mustaches
#assert.eq(t("comment", name: "Ada"), "abc Ada")

// Helpers with path, string, and number parameters, and without parameters
#assert.eq(t("cart.items", count: 1), "1 item")
#assert.eq(t("cart.items", count: 3), "3 items")
#assert.eq(t("helper-literals", name: "Ada"), "\"a b\",1,-2.5,\"Ada\"")
#assert.eq(t("helper-bare", name: "Ada"), "2026-01-01 for Ada")

// Dictionary fields of an `any` argument
#assert.eq(t("field", user: (name: "Ada", "home town": "London")), "Ada (London)")

// Scalars use ASCII minus signs; `float` accepts integers
#assert.eq(
  t("scalars", count: -3, total: 7, price: 2.5, flag: true, date: "2026-01-01", value: none),
  "-3 / 7 / 2.5 / true / 2026-01-01 / ",
)
#assert.eq(
  t("scalars", count: 0, total: 0, price: 2, flag: false, date: "", value: 1.25),
  "0 / 0 / 2 / false /  / 1.25",
)

// Engine none still returns literals
#let plain = translator(
  (version: "1", engine: none, languages: ("en",), arguments: (k: (:)), translations: (en: (k: "{{k}}"))),
  "en",
)
#assert.eq(plain("k"), "{{k}}")

The runtime renders every supported construct.
