//! Config used by the language server.
//!
//! We currently get this config from `initialize` LSP request, which is not the
//! best way to do it, but was the simplest thing we could implement.

use itertools::Itertools;
use lsp_types::ClientCapabilities;
use serde::{de::DeserializeOwned, Deserialize};
use std::{fmt, iter};
use vfs::AbsPathBuf;

// Conventions for configuration keys to preserve maximal extendability without breakage:
//  - Toggles (be it binary true/false or with more options in-between) should almost always suffix as `_enable`
//    This has the benefit of namespaces being extensible, and if the suffix doesn't fit later it can be changed without breakage.
//  - In general be wary of using the namespace of something verbatim, it prevents us from adding subkeys in the future
//  - Don't use abbreviations unless really necessary
//  - foo_command = overrides the subcommand, foo_overrideCommand allows full overwriting, extra args only applies for foo_command

// Defines the server-side configuration of the rust-analyzer. We generate
// *parts* of VS Code's `package.json` config from this. Run `cargo test` to
// re-generate that file.
//
// However, editor specific config, which the server doesn't know about, should
// be specified directly in `package.json`.
//
// To deprecate an option by replacing it with another name use `new_name | `old_name` so that we keep
// parsing the old name.
config_data! {
    struct ConfigData {
        /// The database connection string.
        db_connection_string: Option<String>     = "null",
    }
}

impl Default for ConfigData {
    fn default() -> Self {
        ConfigData::from_json(serde_json::Value::Null, &mut Vec::new())
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    caps: lsp_types::ClientCapabilities,
    root_path: AbsPathBuf,
    data: ConfigData,
    is_visual_studio_code: bool,
}

#[derive(Debug)]
pub struct ConfigError {
    errors: Vec<(String, serde_json::Error)>,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let errors = self.errors.iter().format_with("\n", |(key, e), f| {
            f(key)?;
            f(&": ")?;
            f(e)
        });
        write!(
            f,
            "invalid config value{}:\n{}",
            if self.errors.len() == 1 { "" } else { "s" },
            errors
        )
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn new(
        root_path: AbsPathBuf,
        caps: ClientCapabilities,
        is_visual_studio_code: bool,
    ) -> Self {
        Config {
            caps,
            data: ConfigData::default(),
            root_path,
            is_visual_studio_code,
        }
    }

    pub fn update(&mut self, mut json: serde_json::Value) -> Result<(), ConfigError> {
        tracing::info!("updating config from JSON: {:#}", json);
        if json.is_null() || json.as_object().map_or(false, |it| it.is_empty()) {
            return Ok(());
        }
        let mut errors = Vec::new();
        self.data = ConfigData::from_json(json, &mut errors);
        tracing::debug!("deserialized config data: {:#?}", self.data);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigError { errors })
        }
    }

    pub fn json_schema() -> serde_json::Value {
        ConfigData::json_schema()
    }

    pub fn root_path(&self) -> &AbsPathBuf {
        &self.root_path
    }

    pub fn caps(&self) -> &lsp_types::ClientCapabilities {
        &self.caps
    }
}

macro_rules! try_ {
    ($expr:expr) => {
        || -> _ { Some($expr) }()
    };
}
macro_rules! try_or {
    ($expr:expr, $or:expr) => {
        try_!($expr).unwrap_or($or)
    };
}

macro_rules! try_or_def {
    ($expr:expr) => {
        try_!($expr).unwrap_or_default()
    };
}

impl Config {
    pub fn did_save_text_document_dynamic_registration(&self) -> bool {
        let caps = try_or_def!(self.caps.text_document.as_ref()?.synchronization.clone()?);
        caps.did_save == Some(true) && caps.dynamic_registration == Some(true)
    }

    pub fn did_change_watched_files_dynamic_registration(&self) -> bool {
        try_or_def!(
            self.caps
                .workspace
                .as_ref()?
                .did_change_watched_files
                .as_ref()?
                .dynamic_registration?
        )
    }

    // FIXME: VSCode seems to work wrong sometimes, see https://github.com/microsoft/vscode/issues/193124
    // hence, distinguish it for now.
    pub fn is_visual_studio_code(&self) -> bool {
        self.is_visual_studio_code
    }
}

// Deserialization definitions

macro_rules! create_bool_or_string_de {
    ($ident:ident<$bool:literal, $string:literal>) => {
        fn $ident<'de, D>(d: D) -> Result<(), D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct V;
            impl<'de> serde::de::Visitor<'de> for V {
                type Value = ();

                fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    formatter.write_str(concat!(
                        stringify!($bool),
                        " or \"",
                        stringify!($string),
                        "\""
                    ))
                }

                fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    match v {
                        $bool => Ok(()),
                        _ => Err(serde::de::Error::invalid_value(
                            serde::de::Unexpected::Bool(v),
                            &self,
                        )),
                    }
                }

                fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    match v {
                        $string => Ok(()),
                        _ => Err(serde::de::Error::invalid_value(
                            serde::de::Unexpected::Str(v),
                            &self,
                        )),
                    }
                }

                fn visit_enum<A>(self, a: A) -> Result<Self::Value, A::Error>
                where
                    A: serde::de::EnumAccess<'de>,
                {
                    use serde::de::VariantAccess;
                    let (variant, va) = a.variant::<&'de str>()?;
                    va.unit_variant()?;
                    match variant {
                        $string => Ok(()),
                        _ => Err(serde::de::Error::invalid_value(
                            serde::de::Unexpected::Str(variant),
                            &self,
                        )),
                    }
                }
            }
            d.deserialize_any(V)
        }
    };
}
create_bool_or_string_de!(true_or_always<true, "always">);
create_bool_or_string_de!(false_or_never<false, "never">);

macro_rules! named_unit_variant {
    ($variant:ident) => {
        pub(super) fn $variant<'de, D>(deserializer: D) -> Result<(), D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct V;
            impl<'de> serde::de::Visitor<'de> for V {
                type Value = ();
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str(concat!("\"", stringify!($variant), "\""))
                }
                fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                    if value == stringify!($variant) {
                        Ok(())
                    } else {
                        Err(E::invalid_value(serde::de::Unexpected::Str(value), &self))
                    }
                }
            }
            deserializer.deserialize_str(V)
        }
    };
}

mod de_unit_v {
    named_unit_variant!(all);
    named_unit_variant!(skip_trivial);
    named_unit_variant!(mutable);
    named_unit_variant!(reborrow);
    named_unit_variant!(fieldless);
    named_unit_variant!(with_block);
    named_unit_variant!(decimal);
    named_unit_variant!(hexadecimal);
    named_unit_variant!(both);
}

macro_rules! _config_data {
    (struct $name:ident {
        $(
            $(#[doc=$doc:literal])*
            $field:ident $(| $alias:ident)*: $ty:ty = $default:expr,
        )*
    }) => {
        #[allow(non_snake_case)]
        #[derive(Debug, Clone)]
        struct $name { $($field: $ty,)* }
        impl $name {
            fn from_json(mut json: serde_json::Value, error_sink: &mut Vec<(String, serde_json::Error)>) -> $name {
                $name {$(
                    $field: get_field(
                        &mut json,
                        error_sink,
                        stringify!($field),
                        None$(.or(Some(stringify!($alias))))*,
                        $default,
                    ),
                )*}
            }

            fn json_schema() -> serde_json::Value {
                schema(&[
                    $({
                        let field = stringify!($field);
                        let ty = stringify!($ty);

                        (field, ty, &[$($doc),*], $default)
                    },)*
                ])
            }

        }
    };
}
use _config_data as config_data;

fn get_field<T: DeserializeOwned>(
    json: &mut serde_json::Value,
    error_sink: &mut Vec<(String, serde_json::Error)>,
    field: &'static str,
    alias: Option<&'static str>,
    default: &str,
) -> T {
    // XXX: check alias first, to work around the VS Code where it pre-fills the
    // defaults instead of sending an empty object.
    alias
        .into_iter()
        .chain(iter::once(field))
        .filter_map(move |field| {
            let mut pointer = field.replace('_', "/");
            pointer.insert(0, '/');
            json.pointer_mut(&pointer)
                .map(|it| serde_json::from_value(it.take()).map_err(|e| (e, pointer)))
        })
        .find(Result::is_ok)
        .and_then(|res| match res {
            Ok(it) => Some(it),
            Err((e, pointer)) => {
                tracing::warn!("Failed to deserialize config field at {}: {:?}", pointer, e);
                error_sink.push((pointer, e));
                None
            }
        })
        .unwrap_or_else(|| {
            serde_json::from_str(default).unwrap_or_else(|e| panic!("{e} on: `{default}`"))
        })
}

fn schema(fields: &[(&'static str, &'static str, &[&str], &str)]) -> serde_json::Value {
    let map = fields
        .iter()
        .map(|(field, ty, doc, default)| {
            let name = field.replace('_', ".");
            let name = format!("rust-analyzer.{name}");
            let props = field_props(field, ty, doc, default);
            (name, props)
        })
        .collect::<serde_json::Map<_, _>>();
    map.into()
}

fn field_props(field: &str, ty: &str, doc: &[&str], default: &str) -> serde_json::Value {
    let doc = doc_comment_to_string(doc);
    let doc = doc.trim_end_matches('\n');
    assert!(
        doc.ends_with('.') && doc.starts_with(char::is_uppercase),
        "bad docs for {field}: {doc:?}"
    );
    let default = default.parse::<serde_json::Value>().unwrap();

    let mut map = serde_json::Map::default();
    macro_rules! set {
        ($($key:literal: $value:tt),*$(,)?) => {{$(
            map.insert($key.into(), serde_json::json!($value));
        )*}};
    }
    set!("markdownDescription": doc);
    set!("default": default);

    match ty {
        "bool" => set!("type": "boolean"),
        "usize" => set!("type": "integer", "minimum": 0),
        "String" => set!("type": "string"),
        "Vec<String>" => set! {
            "type": "array",
            "items": { "type": "string" },
        },
        "Vec<PathBuf>" => set! {
            "type": "array",
            "items": { "type": "string" },
        },
        "FxHashSet<String>" => set! {
            "type": "array",
            "items": { "type": "string" },
            "uniqueItems": true,
        },
        "FxHashMap<Box<str>, Box<[Box<str>]>>" => set! {
            "type": "object",
        },
        "FxHashMap<String, SnippetDef>" => set! {
            "type": "object",
        },
        "FxHashMap<String, String>" => set! {
            "type": "object",
        },
        "FxHashMap<Box<str>, usize>" => set! {
            "type": "object",
        },
        "Option<usize>" => set! {
            "type": ["null", "integer"],
            "minimum": 0,
        },
        "Option<String>" => set! {
            "type": ["null", "string"],
        },
        "Option<PathBuf>" => set! {
            "type": ["null", "string"],
        },
        "Option<bool>" => set! {
            "type": ["null", "boolean"],
        },
        "Option<Vec<String>>" => set! {
            "type": ["null", "array"],
            "items": { "type": "string" },
        },
        _ => panic!("missing entry for {ty}: {default}"),
    }

    map.into()
}

fn doc_comment_to_string(doc: &[&str]) -> String {
    doc.iter()
        .map(|it| it.strip_prefix(' ').unwrap_or(it))
        .map(|it| format!("{it}\n"))
        .collect()
}
