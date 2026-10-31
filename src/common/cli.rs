//! Python argparse에서 실제로 사용하는 옵션 의미를 작은 공통 경계로 유지한다.
use std::{collections::BTreeMap, ffi::OsString, fmt, path::PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Flag,
    Value,
    Integer,
    Range(i64, i64),
    Choice(&'static [&'static str]),
    Float,
    Pair,
}
#[derive(Debug, Clone, Copy)]
pub struct OptionSpec {
    pub name: &'static str,
    pub kind: Kind,
    pub group: u8,
    pub required: bool,
}
impl OptionSpec {
    pub const fn new(name: &'static str, kind: Kind) -> Self {
        Self {
            name,
            kind,
            group: 0,
            required: false,
        }
    }
    pub const fn group(mut self, group: u8) -> Self {
        self.group = group;
        self
    }
    pub const fn required(mut self) -> Self {
        self.required = true;
        self
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError(pub String);
impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for UsageError {}

#[derive(Debug, Default)]
pub struct Arguments {
    values: BTreeMap<&'static str, Vec<OsString>>,
    pub help: bool,
}
impl Arguments {
    pub fn has(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }
    pub fn raw(&self, name: &str) -> Option<&OsString> {
        self.values.get(name)?.first()
    }
    pub fn path(&self, name: &str) -> Option<PathBuf> {
        self.raw(name).map(|value| {
            let path = PathBuf::from(value);
            let path: PathBuf = path
                .components()
                .filter(|c| !matches!(c, std::path::Component::CurDir))
                .collect();
            if path.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                path
            }
        })
    }
    pub fn text(&self, name: &str) -> Result<Option<&str>, UsageError> {
        self.raw(name)
            .map(|v| {
                v.to_str()
                    .ok_or_else(|| UsageError(format!("{name}: Unicode text required")))
            })
            .transpose()
    }
    pub fn integer(&self, name: &str) -> Option<serde_json::Number> {
        self.raw(name)?.to_str().and_then(integer)
    }
    pub fn small(&self, name: &str) -> Option<i64> {
        self.integer(name)?.as_i64()
    }
    pub fn float(&self, name: &str) -> Option<f64> {
        self.raw(name)?.to_str().and_then(float)
    }
    pub fn pair(&self, name: &str) -> Option<[i64; 2]> {
        let values = self.values.get(name)?;
        Some([
            integer(values.first()?.to_str()?)?.as_i64()?,
            integer(values.get(1)?.to_str()?)?.as_i64()?,
        ])
    }
    pub fn pair_numbers(&self, name: &str) -> Option<[serde_json::Number; 2]> {
        let values = self.values.get(name)?;
        Some([
            integer(values.first()?.to_str()?)?,
            integer(values.get(1)?.to_str()?)?,
        ])
    }
}
fn integer(text: &str) -> Option<serde_json::Number> {
    // int()는 ASCII U+001c..001f를 str.strip()과 달리 공백으로 받지 않는다.
    let text =
        text.trim_matches(|c: char| c.is_whitespace() && !('\u{1c}'..='\u{1f}').contains(&c));
    if text.starts_with(['\u{1c}', '\u{1d}', '\u{1e}', '\u{1f}'])
        || text.ends_with(['\u{1c}', '\u{1d}', '\u{1e}', '\u{1f}'])
    {
        return None;
    }
    super::numeric_input::integer(text)
}
fn float(text: &str) -> Option<f64> {
    let mut normalized = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            normalized.push(c);
        } else if let Some(n) = super::numeric_input::decimal(c) {
            normalized.push(char::from(b'0' + n as u8));
        } else if c.is_whitespace() {
            normalized.push(' ');
        } else {
            return None;
        }
    }
    let text =
        normalized.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'));
    let chars: Vec<_> = text.chars().collect();
    for (index, c) in chars.iter().enumerate() {
        if *c == '_'
            && (index == 0
                || index + 1 == chars.len()
                || !chars[index - 1].is_ascii_digit()
                || !chars[index + 1].is_ascii_digit())
        {
            return None;
        }
    }
    let compact = text.replace('_', "").to_ascii_lowercase();
    match compact.as_str() {
        "inf" | "+inf" | "infinity" | "+infinity" => Some(f64::INFINITY),
        "-inf" | "-infinity" => Some(f64::NEG_INFINITY),
        "nan" | "+nan" | "-nan" => Some(f64::NAN),
        _ => compact.parse().ok(),
    }
}
fn negative_number(text: &str) -> bool {
    let Some(tail) = text.strip_prefix('-') else {
        return false;
    };
    let mut dot = false;
    let mut digits = 0;
    for c in tail.chars() {
        if c == '.' && !dot {
            dot = true;
        } else if super::numeric_input::decimal(c).is_some() {
            digits += 1;
        } else {
            return false;
        }
    }
    digits > 0 && (!dot || !tail.ends_with('.'))
}
fn option_like(value: &std::ffi::OsStr) -> bool {
    value.as_encoded_bytes().len() > 1
        && value.as_encoded_bytes().starts_with(b"-")
        && !value.to_str().is_some_and(negative_number)
}
fn split_option(value: &std::ffi::OsStr) -> (Option<String>, Option<OsString>) {
    if let Some(text) = value.to_str() {
        return match text.split_once('=') {
            Some((name, argument)) => (Some(name.into()), Some(argument.into())),
            None => (Some(text.into()), None),
        };
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let wide: Vec<_> = value.encode_wide().collect();
        if let Some(index) = wide.iter().position(|c| *c == u16::from(b'=')) {
            return (
                String::from_utf16(&wide[..index]).ok(),
                Some(OsString::from_wide(&wide[index + 1..])),
            );
        }
    }
    (None, None)
}
fn validate(spec: OptionSpec, values: &[OsString]) -> Result<(), UsageError> {
    for value in values {
        let invalid = || {
            UsageError(format!(
                "{}: invalid value '{}'",
                spec.name,
                value.to_string_lossy()
            ))
        };
        match spec.kind {
            Kind::Flag | Kind::Value => {}
            Kind::Choice(choices) => {
                if !choices.iter().any(|c| value == c) {
                    return Err(invalid());
                }
            }
            Kind::Integer | Kind::Range(..) | Kind::Pair => {
                let number = value.to_str().and_then(integer).ok_or_else(invalid)?;
                if let Kind::Range(low, high) = spec.kind
                    && !number.as_i64().is_some_and(|n| (low..=high).contains(&n))
                {
                    return Err(invalid());
                }
            }
            Kind::Float => {
                value.to_str().and_then(float).ok_or_else(invalid)?;
            }
        }
    }
    Ok(())
}
pub fn parse(
    input: impl IntoIterator<Item = OsString>,
    specs: &[OptionSpec],
    required_groups: &[u8],
) -> Result<Arguments, UsageError> {
    let mut input = input.into_iter().peekable();
    let mut supported = specs.to_vec();
    supported.push(OptionSpec::new("--help", Kind::Flag));
    let specs = supported.as_slice();
    let mut parsed = Arguments::default();
    let mut groups = BTreeMap::new();
    let mut unknown = Vec::new();
    while let Some(argument) = input.next() {
        if argument == "--" {
            unknown.push(argument);
            unknown.extend(input);
            break;
        }
        let spelling = argument.to_str();
        if matches!(spelling, Some("--help" | "-h"))
            || spelling.is_some_and(|s| s.starts_with("-h") && s[1..].bytes().all(|c| c == b'h'))
        {
            parsed.help = true;
            return Ok(parsed);
        }
        let (name, explicit) = split_option(&argument);
        let name = name.as_deref();
        let exact = specs.iter().find(|s| Some(s.name) == name).copied();
        let candidates = if exact.is_some() {
            Vec::new()
        } else {
            specs
                .iter()
                .filter(|s| name.is_some_and(|n| n.starts_with("--") && s.name.starts_with(n)))
                .copied()
                .collect::<Vec<_>>()
        };
        let spec = if let Some(exact) = exact {
            exact
        } else if candidates.len() == 1 {
            candidates[0]
        } else if candidates.len() > 1 {
            return Err(UsageError(format!(
                "ambiguous option: {}",
                argument.to_string_lossy()
            )));
        } else {
            unknown.push(argument);
            continue;
        };
        let count = match spec.kind {
            Kind::Flag => 0,
            Kind::Pair => 2,
            _ => 1,
        };
        let mut values = Vec::with_capacity(count);
        if let Some(value) = explicit {
            if count != 1 {
                return Err(UsageError(format!(
                    "{}: expected {count} arguments",
                    spec.name
                )));
            }
            values.push(value);
        } else {
            for _ in 0..count {
                let Some(value) = input.peek().filter(|v| !option_like(v)) else {
                    return Err(UsageError(format!(
                        "{}: expected {count} arguments",
                        spec.name
                    )));
                };
                values.push(value.clone());
                input.next();
            }
        }
        validate(spec, &values)?;
        if spec.name == "--help" {
            parsed.help = true;
            return Ok(parsed);
        }
        if spec.group != 0
            && let Some(previous) = groups.insert(spec.group, spec.name)
            && previous != spec.name
        {
            return Err(UsageError(format!(
                "{}: not allowed with {previous}",
                spec.name
            )));
        }
        parsed.values.insert(spec.name, values);
    }
    if !unknown.is_empty() {
        return Err(UsageError(format!(
            "unrecognized arguments: {}",
            unknown
                .iter()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        )));
    }
    for spec in specs {
        if spec.required && !parsed.has(spec.name) {
            return Err(UsageError(format!("required argument: {}", spec.name)));
        }
    }
    for group in required_groups {
        if !groups.contains_key(group) {
            return Err(UsageError(
                "one of the mutually exclusive arguments is required".into(),
            ));
        }
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_argument_edges() -> Result<(), UsageError> {
        let specs = [
            OptionSpec::new("--number", Kind::Range(1, 12)),
            OptionSpec::new("--north", Kind::Flag).group(1),
            OptionSpec::new("--south", Kind::Flag).group(1),
            OptionSpec::new("--path", Kind::Value),
        ];
        let call = |args: &[&str]| parse(args.iter().map(OsString::from), &specs, &[]);
        let p = call(&[
            "--num=３",
            "--number",
            "+1_2",
            "--path=한글 경로",
            "--north",
            "--north",
        ])?;
        assert_eq!(p.small("--number"), Some(12));
        assert_eq!(p.path("--path"), Some(PathBuf::from("한글 경로")));
        assert!(call(&["--unknown", "--help"])?.help);
        assert!(call(&["--number", "bad", "--help"]).is_err());
        for args in [
            &["--n"][..],
            &["--north", "--south"],
            &["--number", "--path"],
            &["--number", "\u{1c}1"],
            &["--number", "-1"],
            &["--north=true"],
        ] {
            assert!(call(args).is_err(), "{args:?}");
        }
        assert_eq!(
            integer("\u{2003}１２\u{2003}").and_then(|n| n.as_u64()),
            Some(12)
        );
        assert!(float("1__0").is_none());
        assert_eq!(float("３.５e２"), Some(350.0));
        Ok(())
    }
}
