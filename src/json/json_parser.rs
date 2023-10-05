use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub entrypoint: String,
    pub num_args: u64,
    pub type_args: Vec<String>,
    pub hints: bool,
    pub decorators: Vec<String>,
}

/// Function that returns a vector of the args type of the function the user want to fuzz
fn get_type_args(members: &Value) -> Vec<String> {
    let mut type_args = Vec::<String>::new();
    for (_, value) in members
        .as_object()
        .expect("Failed get member type_args as object from json")
    {
        type_args.push(value["cairo_type"].to_string().replace("\"", ""));
    }
    return type_args;
}

/// Function to parse cairo json artifact
pub fn parse_json(data: &String, function_name: &String) -> Option<Function> {
    let data: Value = serde_json::from_str(&data).expect("JSON was not well-formatted");
    let hints = if let Some(field) = data.get("hints") {
        field.as_object().unwrap().len() != 0
    } else {
        false
    };
    if let Some(identifiers) = data.get("identifiers") {
        for (key, value) in identifiers
            .as_object()
            .expect("Failed to get identifier from json")
        {
            let name = key.split(".").last().unwrap().to_string();
            if value["type"] == "function" && &name == function_name {
                let pc = value["pc"].to_string();
                if let Some(identifiers_key) = identifiers.get(format!("{}.Args", key)) {
                    if let (Some(size), Some(members)) =
                        (identifiers_key.get("size"), identifiers_key.get("members"))
                    {
                        return Some(Function {
                            decorators: Vec::new(),
                            entrypoint: pc,
                            hints,
                            name,
                            num_args: size
                                .as_u64()
                                .expect("Failed to get number of arguments from json"),
                            type_args: get_type_args(members),
                        });
                    }
                }
            }
        }
    }
    return None;
}
