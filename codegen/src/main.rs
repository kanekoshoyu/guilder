use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
struct YamlConfig {
    traits: Vec<Trait>,
}

#[derive(Deserialize, Debug)]
struct Trait {
    name: String,
    methods: Vec<Method>,
}

#[derive(Deserialize, Debug)]
struct Method {
    name: String,
    return_type: String,
    args: Vec<Arg>,
}

#[derive(Deserialize, Debug)]
struct Arg {
    name: String,
    #[serde(rename = "type")]
    arg_type: String,
}

fn parse_yaml(file_path: impl AsRef<std::path::Path>) -> YamlConfig {
    let file_content = std::fs::read_to_string(file_path).expect("Failed to read YAML file");
    let config: YamlConfig = serde_yaml::from_str(&file_content).expect("Failed to parse YAML");
    config
}

fn generate_trait_code_rust(config: YamlConfig) -> String {
    let mut code = String::new();

    for tr in config.traits {
        code.push_str(&format!("trait {} {{\n", tr.name));

        for method in tr.methods {
            let args: Vec<String> = method
                .args
                .iter()
                .map(|arg| format!("{}: {}", arg.name, arg.arg_type))
                .collect();
            let args_str = args.join(", ");

            code.push_str(&format!(
                "    fn {}({}) -> {};\n",
                method.name, args_str, method.return_type
            ));
        }

        code.push_str("}\n\n");
    }

    code
}

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
}
fn main() {
    let config = parse_yaml("../trading.yaml"); // Make sure to have the yaml file at the correct path
    let output_dir = "../target/rust/guilder.rs";
    let code = generate_trait_code_rust(config);
    println!("{}", code);
}
