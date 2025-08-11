### Globetrotter rust example

This example uses a `build.rs` script to generate the json translations and corresponding
rust bindings.

The build script is similar to the following configuration file, except it stores the 
generated files in `OUT_DIR`.

```json
version: 1
config:
  languages: [en, de, fr]
  engine: handlebars
  strict: true
  check_templates: true
  inputs:
    - "translations.toml"
  outputs:
    json:
      - ./output/{{language}}.json
    rust:
      - ./translations.rs
```
