use serde::{de, Deserialize, Deserializer, Serialize};
use std::fmt::{self};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
enum ProgrammingLanguage {
    Rust,
    Python,
}
impl ProgrammingLanguage {
    pub fn file_extension(&self) -> &'static str {
        match self {
            Self::Rust => "rs",
            Self::Python => "py",
        }
    }
    pub fn alias(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
struct YamlConfig {
    traits: Vec<Trait>,
    structs: Vec<Struct>,
    #[serde(default)]
    enums: Vec<Enum>,
}

#[derive(Deserialize, Debug, Clone)]
struct Enum {
    name: String,
    description: Option<String>,
    values: Vec<EnumValue>,
}

#[derive(Deserialize, Debug, Clone)]
struct EnumValue {
    name: String,
    description: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct Trait {
    name: String,
    description: Option<String>,
    #[serde(default)]
    r#async: bool,
    methods: Vec<TraitMethod>,
}

#[derive(Deserialize, Debug, Clone)]
struct TraitMethod {
    name: String,
    description: Option<String>,
    return_type: ValueType,
    #[serde(default)]
    // default to empty vec when not provided
    args: Vec<TraitMethodArgument>,
}

#[derive(Deserialize, Debug, Clone)]
struct TraitMethodArgument {
    name: String,
    #[serde(rename = "type")]
    arg_type: ValueType,
}

#[derive(Deserialize, Debug, Clone)]
struct Struct {
    name: String,
    description: String,
    values: Vec<StructValue>,
}

#[derive(Deserialize, Debug, Clone)]
struct StructValue {
    name: String,
    #[serde(rename = "type")]
    value_type: ValueType,
}

// TODO implement the serde for ValueType::List
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
enum ValueType {
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    F32,
    F64,
    Bool,
    Char,
    #[serde(rename = "String")]
    String,
    #[serde(rename = "()")]
    Unit,
    // Represents a lazy pull-based sequence, from Iterator<T: ValueType>
    Iter(Box<ValueType>),
    // Represents an async push-based stream (e.g. WebSocket), from Stream<T: ValueType>
    Stream(Box<ValueType>),
    // Represents arrays of a specific type, from Vec<T: ValueType>
    List(Box<ValueType>),
    // Represents arrays of a specific type, from HashMap<T: ValueType, U: ValueType>
    Map {
        key_type: Box<ValueType>,
        value_type: Box<ValueType>,
    },
    // types that are defined in the yaml itself, and should be printed directly in outcome
    CustomType(String),
}
impl ValueType {
    pub fn to_string(&self, language: ProgrammingLanguage) -> String {
        self.to_string_async(language, false)
    }

    pub fn to_string_async(&self, language: ProgrammingLanguage, is_async: bool) -> String {
        match language {
            ProgrammingLanguage::Rust => match self {
                Self::I8 => "i8".into(),
                Self::I16 => "i16".into(),
                Self::I32 => "i32".into(),
                Self::I64 => "i64".into(),
                Self::I128 => "i128".into(),
                Self::U8 => "u8".into(),
                Self::U16 => "u16".into(),
                Self::U32 => "u32".into(),
                Self::U64 => "u64".into(),
                Self::U128 => "u128".into(),
                Self::F32 => "f32".into(),
                Self::F64 => "f64".into(),
                Self::Bool => "bool".into(),
                Self::Char => "char".into(),
                Self::String => "String".into(),
                Self::Unit => "()".into(),
                Self::Stream(item_type) => {
                    format!("impl Stream<Item = {}>", item_type.to_string_async(ProgrammingLanguage::Rust, is_async))
                }
                Self::Iter(item_type) => {
                    format!("impl Iterator<Item = {}>", item_type.to_string_async(ProgrammingLanguage::Rust, is_async))
                }
                Self::List(item_type) => {
                    format!("Vec<{}>", item_type.to_string_async(ProgrammingLanguage::Rust, is_async))
                }
                Self::Map { key_type, value_type } => {
                    format!(
                        "HashMap<{}, {}>",
                        key_type.to_string_async(ProgrammingLanguage::Rust, is_async),
                        value_type.to_string_async(ProgrammingLanguage::Rust, is_async)
                    )
                }
                Self::CustomType(type_str) => type_str.clone(),
            },
            ProgrammingLanguage::Python => match self {
                Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::I128
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
                | Self::U128 => "int".into(),
                Self::F32 | Self::F64 => "float".into(),
                Self::Bool => "bool".into(),
                Self::Char | Self::String => "str".into(),
                Self::Unit => "None".to_string(),
                Self::Stream(item_type) => {
                    // Stream is always async push — maps to AsyncIterator regardless of trait flag
                    let inner = item_type.to_string_async(ProgrammingLanguage::Python, is_async);
                    format!("AsyncIterator[{}]", inner)
                }
                Self::Iter(item_type) => {
                    let inner = item_type.to_string_async(ProgrammingLanguage::Python, is_async);
                    if is_async {
                        format!("AsyncIterator[{}]", inner)
                    } else {
                        format!("Iterator[{}]", inner)
                    }
                }
                Self::List(item_type) => {
                    format!("list[{}]", item_type.to_string_async(ProgrammingLanguage::Python, is_async))
                }
                Self::Map { key_type, value_type } => {
                    format!(
                        "dict[{}, {}]",
                        key_type.to_string_async(ProgrammingLanguage::Python, is_async),
                        value_type.to_string_async(ProgrammingLanguage::Python, is_async)
                    )
                }
                Self::CustomType(type_str) => type_str.clone(),
            },
        }
    }
}

impl<'de> Deserialize<'de> for ValueType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ValueTypeVisitor;

        impl<'de> de::Visitor<'de> for ValueTypeVisitor {
            type Value = ValueType;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string representing a Rust type, like Vec<i64>")
            }

            fn visit_str<E>(self, value: &str) -> Result<ValueType, E>
            where
                E: de::Error,
            {
                parse_value_type(value).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(ValueTypeVisitor)
    }
}

/// Find the position of the first comma at angle-bracket depth 0.
/// This correctly handles nested generics like `HashMap<Vec<i32>, f64>`.
fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn parse_value_type(value: &str) -> Result<ValueType, String> {
    if value.starts_with("Stream<") && value.ends_with('>') {
        let inner_type_str = &value[7..value.len() - 1];
        let inner_type = parse_value_type(inner_type_str)?;
        Ok(ValueType::Stream(Box::new(inner_type)))
    } else if value.starts_with("Iterator<") && value.ends_with('>') {
        let inner_type_str = &value[9..value.len() - 1];
        let inner_type = parse_value_type(inner_type_str)?;
        Ok(ValueType::Iter(Box::new(inner_type)))
    } else if value.starts_with("Vec<") && value.ends_with('>') {
        let inner_type_str = &value[4..value.len() - 1];
        let inner_type = parse_value_type(inner_type_str)?;
        Ok(ValueType::List(Box::new(inner_type)))
    } else if value.starts_with("HashMap<") && value.ends_with('>') {
        let inner = &value[8..value.len() - 1];
        let comma_pos = find_top_level_comma(inner)
            .ok_or_else(|| format!("HashMap type missing comma: {}", value))?;
        let key_type_str = inner[..comma_pos].trim();
        let value_type_str = inner[comma_pos + 1..].trim();
        let key_type = parse_value_type(key_type_str)?;
        let value_type = parse_value_type(value_type_str)?;
        Ok(ValueType::Map {
            key_type: Box::new(key_type),
            value_type: Box::new(value_type),
        })
    } else {
        match value {
            "i8" => Ok(ValueType::I8),
            "i16" => Ok(ValueType::I16),
            "i32" => Ok(ValueType::I32),
            "i64" => Ok(ValueType::I64),
            "i128" => Ok(ValueType::I128),
            "u8" => Ok(ValueType::U8),
            "u16" => Ok(ValueType::U16),
            "u32" => Ok(ValueType::U32),
            "u64" => Ok(ValueType::U64),
            "u128" => Ok(ValueType::U128),
            "f32" => Ok(ValueType::F32),
            "f64" => Ok(ValueType::F64),
            "bool" => Ok(ValueType::Bool),
            "char" => Ok(ValueType::Char),
            "String" => Ok(ValueType::String),
            "()" => Ok(ValueType::Unit),
            type_str => Ok(ValueType::CustomType(type_str.to_string())),
        }
    }
}

fn parse_yaml(file_path: impl AsRef<std::path::Path>) -> YamlConfig {
    let file_content = std::fs::read_to_string(file_path).expect("Failed to read YAML file");
    let config: YamlConfig = serde_yaml::from_str(&file_content).expect("Failed to parse YAML");
    config
}

fn codegen_str_rust(config: YamlConfig) -> String {
    let language = ProgrammingLanguage::Rust;
    let mut code = String::new();

    // dependencies
    code.push_str("use std::collections::HashMap;\n");
    code.push_str("use futures_core::Stream;\n\n");

    // enums
    for en in &config.enums {
        if let Some(desc) = &en.description {
            code.push_str(&format!("/// {}\n", desc));
        }
        code.push_str("#[derive(Debug, Clone, PartialEq)]\n");
        code.push_str(&format!("pub enum {} {{\n", en.name));
        for value in &en.values {
            if let Some(desc) = &value.description {
                code.push_str(&format!("\t/// {}\n", desc));
            }
            code.push_str(&format!("\t{},\n", value.name));
        }
        code.push_str("}\n\n");
    }

    // structs
    for st in config.structs {
        code.push_str(&format!("/// {}\n", st.description));
        code.push_str("#[derive(Debug, Clone)]\n");
        code.push_str(&format!("pub struct {} {{\n", st.name));

        for value in st.values {
            code.push_str(&format!(
                "\tpub {}: {},\n",
                value.name,
                value.value_type.to_string(ProgrammingLanguage::Rust)
            ));
        }

        code.push_str("}\n\n");
    }

    // traits
    for tr in config.traits {
        if let Some(description) = &tr.description {
            code.push_str(&format!("/// {}\n", description));
        }
        if tr.r#async {
            code.push_str("#[allow(async_fn_in_trait)]\n");
        }
        code.push_str(&format!("pub trait {} {{\n", tr.name));

        for method in tr.methods {
            let mut args: Vec<String> = vec!["&self".to_string()];
            args.extend(
                method
                    .args
                    .iter()
                    .map(|arg| format!("{}: {}", arg.name, arg.arg_type.to_string_async(language, tr.r#async))),
            );
            let args_str = args.join(", ");

            if let Some(description) = method.description {
                code.push_str(&format!("\t/// {}\n", description));
            }
            let fn_keyword = if tr.r#async { "async fn" } else { "fn" };
            code.push_str(&format!(
                "\t{} {}({}) -> {};\n",
                fn_keyword,
                method.name,
                args_str,
                method.return_type.to_string_async(language, tr.r#async)
            ));
        }

        code.push_str("}\n\n");
    }

    code
}

fn codegen_str_python(config: YamlConfig) -> String {
    let language = ProgrammingLanguage::Python;
    let mut code = String::new();

    let has_async = config.traits.iter().any(|t| t.r#async);

    // Import the necessary ABC modules at the top
    code.push_str("from abc import ABC, abstractmethod\n");
    code.push_str("from enum import Enum\n");
    if has_async {
        code.push_str("from typing import Iterator, AsyncIterator\n");
    } else {
        code.push_str("from typing import Iterator\n");
    }
    code.push_str("\n");

    // enums
    for en in &config.enums {
        code.push_str(&format!("class {}(Enum):\n", en.name));
        if let Some(desc) = &en.description {
            code.push_str(&format!("\t\"\"\"{}\"\"\"\n", desc));
        }
        for (i, value) in en.values.iter().enumerate() {
            code.push_str(&format!("\t{} = {}\n", value.name, i + 1));
        }
        code.push_str("\n");
    }

    // structs
    for st in config.structs {
        code.push_str(&format!("class {}:\n", st.name));
        code.push_str(&format!("\t\"\"\"{}\"\"\"\n", st.description));
        code.push_str("\tdef __init__(self");
        let mut field_definitions = String::new();
        let mut init_body = String::new();
        for field in st.values {
            field_definitions.push_str(&format!(
                ", {}: {}",
                field.name,
                field.value_type.to_string(ProgrammingLanguage::Python)
            ));
            init_body.push_str(&format!("\t\tself.{} = {}\n", field.name, field.name));
        }

        code.push_str(&format!("{}):\n", field_definitions));
        code.push_str(&init_body);
        code.push_str("\n");
    }

    // traits
    for tr in config.traits {
        code.push_str(&format!("class {}(ABC):\n", tr.name));
        if let Some(description) = &tr.description {
            code.push_str(&format!("\t\"\"\"{}\"\"\"\n", description));
        }
        for method in tr.methods {
            let mut args: Vec<String> = vec!["self".to_string()];
            args.extend(
                method
                    .args
                    .iter()
                    .map(|arg| format!("{}: {}", arg.name, arg.arg_type.to_string_async(language, tr.r#async))),
            );
            let args_str = args.join(", ");

            code.push_str("\t@abstractmethod\n");
            let def_keyword = if tr.r#async { "async def" } else { "def" };
            code.push_str(&format!(
                "\t{} {}({}) -> {}:\n",
                def_keyword,
                method.name,
                args_str,
                method.return_type.to_string_async(language, tr.r#async)
            ));
            if let Some(description) = method.description {
                code.push_str(&format!("\t\t\"\"\"{}\"\"\"\n", description));
            }
            code.push_str("\t\tpass\n\n");
        }

        code.push_str("\n");
    }

    code
}

fn codegen(
    config: YamlConfig,
    language: ProgrammingLanguage,
    output_path: impl AsRef<std::path::Path>,
) -> Result<(), std::io::Error> {
    let codegen_str = match language {
        ProgrammingLanguage::Rust => codegen_str_rust(config),
        ProgrammingLanguage::Python => codegen_str_python(config),
    };
    let output_path = match language {
        ProgrammingLanguage::Rust => format!(
            "{}/{}/src/guilder_abstraction.{}",
            output_path.as_ref().display(),
            language.alias(),
            language.file_extension()
        ),
        ProgrammingLanguage::Python => format!(
            "{}/{}/guilder_abstraction.{}",
            output_path.as_ref().display(),
            language.alias(),
            language.file_extension()
        ),
    };

    let output_path = std::path::Path::new(&output_path);
    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    if !output_path.exists() {
        std::fs::File::create(output_path)?;
    }
    std::fs::write(output_path, codegen_str)
}

fn main() {
    let config = parse_yaml("../trading.yaml");
    let output_path = "../target";
    let languages = [ProgrammingLanguage::Rust, ProgrammingLanguage::Python];
    // let languages = [ProgrammingLanguage::Python];
    for language in languages {
        if let Err(e) = codegen(config.clone(), language, output_path) {
            println!("error: {e}");
        }
    }
}
