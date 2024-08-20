use serde::{Deserialize, Serialize};

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
    methods: Vec<Method>,
}

#[derive(Deserialize, Debug, Clone)]
struct Method {
    name: String,
    return_type: ValueType,
    args: Vec<Argument>,
}

#[derive(Deserialize, Debug, Clone)]
struct Argument {
    name: String,
    #[serde(rename = "type")]
    arg_type: ValueType,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
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
    Unit,  // Corresponds to ()
    Array, // Fixed-size arrays like [i32; 3]
    Slice, // Dynamically sized arrays like &[i32]
}
impl ValueType {
    pub fn to_string(&self, language: ProgrammingLanguage) -> &'static str {
        match language {
            ProgrammingLanguage::Rust => match self {
                Self::I8 => "i8",
                Self::I16 => "i16",
                Self::I32 => "i32",
                Self::I64 => "i64",
                Self::I128 => "i128",
                Self::U8 => "u8",
                Self::U16 => "u16",
                Self::U32 => "u32",
                Self::U64 => "u64",
                Self::U128 => "u128",
                Self::F32 => "f32",
                Self::F64 => "f64",
                Self::Bool => "bool",
                Self::Char => "char",
                Self::String => "String",
                Self::Unit => "unit",
                Self::Array => "array",
                Self::Slice => "slice",
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
                | Self::U128 => "int",
                Self::F32 | Self::F64 => "float",
                Self::Bool => "bool",
                Self::Char | Self::String => "str", // Python doesn't have a single-character type, so `str` is used
                Self::Unit => "None",               // Python equivalent of Rust's unit type ()
                Self::Array | Self::Slice => "list", // Python lists correspond to Rust arrays and slices
            },
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

    for tr in config.traits {
        code.push_str(&format!("trait {} {{\n", tr.name));

        for method in tr.methods {
            let args: Vec<String> = method
                .args
                .iter()
                .map(|arg| format!("{}: {}", arg.name, arg.arg_type.to_string(language)))
                .collect();
            let args_str = args.join(", ");

            code.push_str(&format!(
                "    fn {}({}) -> {};\n",
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

    for tr in config.traits {
        code.push_str(&format!("class {}:\n", tr.name));

        for method in tr.methods {
            let args: Vec<String> = method
                .args
                .iter()
                .map(|arg| format!("{}: {}", arg.name, arg.arg_type.to_string(language)))
                .collect();
            let args_str = args.join(", ");

            code.push_str(&format!(
                "    def {}(self, {}) -> {}:\n",
                method.name,
                args_str,
                method.return_type.to_string(language)
            ));
            code.push_str("        pass\n\n");
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
    let output_path = format!(
        "{}/{}/guilder.{}",
        output_path.as_ref().display(),
        language.alias(),
        language.file_extension()
    );

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
    let config = parse_yaml("../trading.yaml"); // Make sure to have the yaml file at the correct path
    let output_path = "../target";
    // let languages = [ProgrammingLanguage::Rust, ProgrammingLanguage::Python];
    let languages = [ProgrammingLanguage::Python];
    for language in languages {
        if let Err(e) = codegen(config.clone(), language, output_path) {
            println!("error: {e}");
        }
    }
}
