// Translation data for a Typst rendering runtime; this module renders nothing itself.
// The globetrotter Typst package renders it: translator(module, lang) returns t(key, ..args).
//
// version: the format version, shared with globetrotter's JSON output.
// engine: the template engine name, such as "handlebars", or none.
// languages: language codes in configured order.
// arguments: key -> (argument name -> type) for every key.
//   Types are globetrotter's argument type names: any, string, number, integer, float,
//   isodatetime, boolean.
//   A runtime rejects unknown keys and missing, unexpected, or mistyped arguments with it.
// translations: language -> key -> source string, with the same keys as arguments.
//   A key without arguments is a literal to return verbatim.
//   Every other key is a template for engine.
