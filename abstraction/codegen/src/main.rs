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
}

#[derive(Deserialize, Debug, Clone)]
struct Trait {
    name: String,
    description: Option<String>,
    methods: Vec<Method>,
}

#[derive(Deserialize, Debug, Clone)]
struct Method {
    name: String,
    description: Option<String>,
    return_type: ValueType,
    #[serde(default)]
    // default to empty vec when not provided
    args: Vec<Argument>,
}

#[derive(Deserialize, Debug, Clone)]
struct Argument {
    name: String,
    #[serde(rename = "type")]
    arg_type: ValueType,
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
    // Represents arrays of a specific type, from Vec<T: ValueType>
    List(Box<ValueType>),
    // Represents arrays of a specific type, from HashMap<T: ValueType, U: ValueType>
    Map {
        key_type: Box<ValueType>,
        value_type: Box<ValueType>,
    },
}
impl ValueType {
    pub fn to_string(&self, language: ProgrammingLanguage) -> String {
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
                Self::Unit => "unit".into(),
                Self::List(item_type) => {
                    format!("Vec<{}>", item_type.to_string(ProgrammingLanguage::Rust))
                }
                Self::Map {
                    key_type,
                    value_type,
                } => {
                    format!(
                        "HashMap<{}, {}>",
                        key_type.to_string(ProgrammingLanguage::Rust),
                        value_type.to_string(ProgrammingLanguage::Rust)
                    )
                }
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
                Self::Char | Self::String => "str".into(), // Python has no single-character type, so `str` is used
                Self::Unit => "None".to_string(), // Python equivalent of Rust's unit type ()
                Self::List(x) => format!("list[{}]", x.to_string(ProgrammingLanguage::Python)),
                Self::Map {
                    key_type,
                    value_type,
                } => {
                    format!(
                        "dict[{}, {}]",
                        key_type.to_string(ProgrammingLanguage::Python),
                        value_type.to_string(ProgrammingLanguage::Python)
                    )
                }
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

fn parse_value_type(value: &str) -> Result<ValueType, String> {
    // TODO make this neater with the user of match
    if value.starts_with("Vec<") && value.ends_with('>') {
        let inner_type_str = &value[4..value.len() - 1];
        let inner_type = parse_value_type(inner_type_str)?;
        Ok(ValueType::List(Box::new(inner_type)))
    } else if value.starts_with("HashMap<") && value.ends_with('>') && value.contains(',') {
        let comma_pos = value.find(',').unwrap();
        let key_type_str = &value[8..comma_pos];
        let value_type_str = &value[comma_pos + 2..value.len() - 1];
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
            _ => Err(format!("Unsupported type: {}", value)),
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
    code.push_str("use std::collections::HashMap;\n\n");

    for tr in config.traits {
        if let Some(description) = tr.description {
            code.push_str(&format!("/// {}\n", description));
        }
        code.push_str(&format!("pub trait {} {{\n", tr.name));

        for method in tr.methods {
            // Create the arguments list with &self included
            let mut args: Vec<String> = vec!["&self".to_string()]; // Add &self as the first argument
            args.extend(
                method
                    .args
                    .iter()
                    .map(|arg| format!("{}: {}", arg.name, arg.arg_type.to_string(language))),
            );
            let args_str = args.join(", ");

            if let Some(description) = method.description {
                code.push_str(&format!("\t/// {}\n", description));
            }
            code.push_str(&format!(
                "\tfn {}({}) -> {};\n",
                method.name,
                args_str,
                method.return_type.to_string(language)
            ));
        }

        code.push_str("}\n\n");
    }

    code
}

fn codegen_str_python(config: YamlConfig) -> String {
    let language = ProgrammingLanguage::Python;
    let mut code = String::new();

    // Import the necessary ABC modules at the top
    code.push_str("from abc import ABC, abstractmethod\n\n");

    for tr in config.traits {
        // Define the class and indicate it inherits from ABC
        code.push_str(&format!("class {}(ABC):\n", tr.name));
        if let Some(description) = tr.description {
            code.push_str(&format!("\t\"\"\"{}\"\"\"\n", description));
        }
        for method in tr.methods {
            let mut args: Vec<String> = vec!["self".to_string()]; // Add &self as the first argument
            args.extend(
                method
                    .args
                    .iter()
                    .map(|arg| format!("{}: {}", arg.name, arg.arg_type.to_string(language))),
            );
            let args_str = args.join(", ");

            // Add the abstractmethod decorator
            code.push_str("\t@abstractmethod\n");
            code.push_str(&format!(
                "\tdef {}({}) -> {}:\n",
                method.name,
                args_str,
                method.return_type.to_string(language)
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
