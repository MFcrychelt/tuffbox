use pest::Parser;
use pest_derive::Parser;
use serde_json::{Map, Value};

#[derive(Parser)]
#[grammar = "ftb.pest"]
pub struct FtbParser;

pub fn parse_snbt_to_json(input: &str) -> Result<Value, String> {
    let mut parsed = FtbParser::parse(Rule::file, input)
        .map_err(|e| format!("SNBT parse error: {}", e))?;

    let file_pair = parsed.next().unwrap();

    let mut root_map = Map::new();

    for pair in file_pair.into_inner() {
        match pair.as_rule() {
            Rule::object => {
                return Ok(parse_value(pair));
            }
            Rule::pair => {
                let (k, v) = parse_pair(pair);
                root_map.insert(k, v);
            }
            Rule::EOI => break,
            _ => {}
        }
    }

    Ok(Value::Object(root_map))
}

fn parse_pair(pair: pest::iterators::Pair<Rule>) -> (String, Value) {
    let mut inner = pair.into_inner();
    let key_pair = inner.next().unwrap();
    let value_pair = inner.next().unwrap();

    let key_rule = key_pair.clone().into_inner().next().unwrap().as_rule();
    let key = match key_rule {
        Rule::string => parse_string(key_pair.into_inner().next().unwrap()),
        _ => key_pair.as_str().to_string(),
    };

    (key, parse_value(value_pair))
}

fn parse_value(pair: pest::iterators::Pair<Rule>) -> Value {
    match pair.as_rule() {
        Rule::object => {
            let mut map = Map::new();
            for inner_pair in pair.into_inner() {
                let (k, v) = parse_pair(inner_pair);
                map.insert(k, v);
            }
            Value::Object(map)
        }
        Rule::array => {
            let mut arr = Vec::new();
            for inner_pair in pair.into_inner() {
                arr.push(parse_value(inner_pair));
            }
            Value::Array(arr)
        }
        Rule::string => Value::String(parse_string(pair)),
        Rule::unquoted_string => Value::String(pair.as_str().to_string()),
        Rule::boolean => Value::Bool(pair.as_str() == "true"),
        Rule::number => {
            let num_str = pair.as_str();
            let clean_num = num_str.trim_end_matches(|c: char| matches!(c, 'b' | 'B' | 's' | 'S' | 'l' | 'L' | 'f' | 'F' | 'd' | 'D'));

            if let Ok(n) = clean_num.parse::<i64>() {
                Value::Number(n.into())
            } else if let Ok(f) = clean_num.parse::<f64>() {
                Value::Number(serde_json::Number::from_f64(f).unwrap())
            } else {
                Value::String(num_str.to_string())
            }
        }
        _ => Value::Null,
    }
}

fn parse_string(pair: pest::iterators::Pair<Rule>) -> String {
    let inner = pair.into_inner().next().unwrap();
    inner.as_str().replace("\\\"", "\"")
}

pub fn json_to_snbt(value: &Value) -> String {
    to_snbt_string(value, 0)
}

fn to_snbt_string(value: &Value, indent: usize) -> String {
    let current_pad = "  ".repeat(indent);
    let inner_pad = "  ".repeat(indent + 1);

    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => {
            if n.is_f64() {
                format!("{}d", n)
            } else {
                n.to_string()
            }
        }
        Value::String(s) => {
            if s.starts_with("[I;") || s.starts_with("[B;") || s.starts_with("[L;") {
                return s.clone();
            }
            format!("\"{}\"", s.replace("\"", "\\\""))
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                return "[]".to_string();
            }
            let mut out = String::from("[\n");
            for val in arr {
                out.push_str(&inner_pad);
                out.push_str(&to_snbt_string(val, indent + 1));
                out.push('\n');
            }
            out.push_str(&current_pad);
            out.push(']');
            out
        }
        Value::Object(map) => {
            if let (Some(Value::String(ty)), Some(Value::Array(items))) =
                (map.get("__snbtArray"), map.get("values"))
            {
                if ty.len() == 1 && "BILFD".contains(ty.as_str()) {
                    if items.is_empty() {
                        return format!("[{ty};]");
                    }
                    let mut out = format!("[{ty};\n");
                    for val in items {
                        out.push_str(&inner_pad);
                        out.push_str(&to_snbt_string(val, indent + 1));
                        out.push('\n');
                    }
                    out.push_str(&current_pad);
                    out.push(']');
                    return out;
                }
            }
            if map.is_empty() {
                return "{}".to_string();
            }
            let mut out = String::from("{\n");
            for (k, v) in map {
                out.push_str(&inner_pad);
                if is_valid_unquoted_key(k) {
                    out.push_str(k);
                } else {
                    out.push_str(&format!("\"{}\"", k.replace("\"", "\\\"")));
                }
                out.push_str(": ");
                out.push_str(&to_snbt_string(v, indent + 1));
                out.push('\n');
            }
            out.push_str(&current_pad);
            out.push('}');
            out
        }
    }
}

fn is_valid_unquoted_key(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_to_snbt_emits_true_false() {
        let out = json_to_snbt(&json!({ "a": true, "b": false }));
        assert!(out.contains("true"), "{out}");
        assert!(out.contains("false"), "{out}");
        assert!(!out.contains("1b"), "{out}");
        assert!(!out.contains("0b"), "{out}");
    }

    #[test]
    fn json_to_snbt_emits_typed_arrays() {
        let v = json!({
            "__snbtArray": "I",
            "values": [1, -2, 3]
        });
        let out = json_to_snbt(&v);
        assert!(out.starts_with("[I;"), "{out}");
        assert!(out.contains('1'), "{out}");
        assert!(out.contains("-2"), "{out}");
    }
}
