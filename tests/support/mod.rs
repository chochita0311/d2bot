//! 전환 전 구현에서 관찰한 고정 기대값. 현재 Rust 구현으로 정답을 생성하지 않는다.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Clone, Copy)]
enum Role {
    Config,
    Files,
    Alias,
}
struct Input {
    path: PathBuf,
    role: Role,
}
pub(crate) struct Case {
    replacements: Vec<(String, String)>,
    inputs: Vec<Input>,
    invalid_config: bool,
}
impl Case {
    pub(crate) fn new(request: &Value) -> Self {
        let root = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
        let mut case = Self {
            replacements: vec![
                (root.clone(), "$ROOT".into()),
                (format!("//?/{root}"), "$EXTROOT".into()),
            ],
            inputs: vec![],
            invalid_config: false,
        };
        case.collect(request, &root, None);
        case.replacements
            .sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
        case
    }
    fn collect(&mut self, value: &Value, root: &str, role: Option<Role>) {
        match value {
            Value::String(text) if role.is_some() => {
                let slash = text.replace('\\', "/");
                let path = Path::new(text);
                let path = if path.is_absolute() {
                    path.to_owned()
                } else {
                    Path::new(root).join(path)
                };
                if matches!(role, Some(Role::Config))
                    && (text.is_empty()
                        || text == "."
                        || path.canonicalize().ok().is_some_and(|p| {
                            Path::new(root).canonicalize().ok().as_ref() == Some(&p)
                        }))
                {
                    self.invalid_config = true;
                }
                if !text.is_empty()
                    && text != "."
                    && path != Path::new(root)
                    && path.exists()
                    && let Some(role) = role
                    && !matches!(role, Role::Alias)
                {
                    self.inputs.push(Input { path, role });
                }
                let plain = slash.strip_prefix("//?/").unwrap_or(&slash);
                let default_tmp = format!("{root}/target/tmp");
                let tmp = option_env!("CARGO_TARGET_TMPDIR")
                    .unwrap_or(&default_tmp)
                    .replace('\\', "/");
                let tmp = tmp.strip_prefix("//?/").unwrap_or(&tmp);
                let prefix = format!("{}/", tmp.trim_end_matches('/'));
                if let Some(rest) = plain.strip_prefix(&prefix) {
                    let owner = rest.split('/').next().unwrap_or_default();
                    let from = format!(
                        "{}{prefix}{owner}",
                        if slash.starts_with("//?/") {
                            "//?/"
                        } else {
                            ""
                        }
                    );
                    if !self.replacements.iter().any(|(old, _)| old == &from) {
                        self.replacements
                            .push((from, format!("$TEMP{}", self.replacements.len() - 2)));
                    }
                }
            }
            Value::Array(values) => {
                for item in values {
                    self.collect(item, root, role);
                }
            }
            Value::Object(values) => {
                for (key, item) in values {
                    if key == "arguments"
                        && let Some(args) = item.as_array()
                    {
                        let mut previous = "";
                        for arg in args {
                            if let Some(text) = arg.as_str() {
                                let (flag, value) =
                                    text.split_once('=').unwrap_or((previous, text));
                                let read = match flag {
                                    "--config" => Role::Config,
                                    "--image" | "--frame" | "--scenario" | "--manifest"
                                    | "--calibration" | "--state" => Role::Files,
                                    _ => Role::Alias,
                                };
                                self.collect(&Value::String(value.to_owned()), root, Some(read));
                                previous = if text.starts_with("--") { text } else { "" };
                            }
                        }
                    } else {
                        let role = match key.as_str() {
                            "config" | "config_path" => Some(Role::Config),
                            "directory" | "overlays" => Some(Role::Files),
                            "path" | "paths" | "template_path" | "frame" | "frames" | "image"
                            | "images" | "manifest" | "calibration" | "evidence"
                            | "rust_directory" | "rust_overlays" => Some(Role::Files),
                            _ => None,
                        };
                        self.collect(item, root, role);
                    }
                }
            }
            _ => {}
        }
    }
    fn text(&self, text: &str, restore: bool) -> String {
        let escape = |text: &str| text.replace('\u{1e}', "\u{1e}E").replace('$', "\u{1e}D");
        let mut value = if restore {
            text.to_owned()
        } else {
            escape(text)
        };
        for (from, to) in &self.replacements {
            if restore {
                value = value.replace(
                    &format!("{to}:repr"),
                    &from.replace('/', "\\").replace('\\', "\\\\"),
                );
                value = value.replace(&format!("{to}:backslash"), &from.replace('/', "\\"));
                value = value.replace(to, from);
            } else {
                value = value.replace(
                    &escape(&from.replace('/', "\\").replace('\\', "\\\\")),
                    &format!("{to}:repr"),
                );
                value = value.replace(
                    &escape(&from.replace('/', "\\")),
                    &format!("{to}:backslash"),
                );
                value = value.replace(&escape(from), to);
            }
        }
        if restore {
            value.replace("\u{1e}D", "$").replace("\u{1e}E", "\u{1e}")
        } else {
            value
        }
    }
    pub(crate) fn value(&self, value: &Value, restore: bool) -> Value {
        match value {
            Value::String(text) => Value::String(self.text(text, restore)),
            Value::Array(items) => {
                Value::Array(items.iter().map(|v| self.value(v, restore)).collect())
            }
            Value::Object(items) => Value::Object(
                items
                    .iter()
                    .map(|(k, v)| (self.text(k, restore), self.value(v, restore)))
                    .collect(),
            ),
            _ => value.clone(),
        }
    }
    pub(crate) fn identity(&self, family: &str, request: &Value) -> Result<Value, Box<dyn Error>> {
        if self.invalid_config {
            return Err(
                "baseline cases must name a bounded config file/directory, not the checkout root"
                    .into(),
            );
        }
        for field in ["config_path", "config"] {
            if request
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(|s| s.is_empty() || s == ".")
            {
                return Err("baseline cases must name an explicit config file/directory".into());
            }
        }
        let mut inputs = vec![];
        for input in &self.inputs {
            self.files(&input.path, matches!(input.role, Role::Config), &mut inputs)?;
        }
        inputs.sort_by_key(|v| v["path"].as_str().unwrap_or_default().to_owned());
        inputs.dedup();
        Ok(json!({"family":family,"request":self.value(request,false),"inputs":inputs}))
    }
    fn files(
        &self,
        path: &Path,
        json_only: bool,
        inputs: &mut Vec<Value>,
    ) -> Result<(), Box<dyn Error>> {
        if path.is_dir() {
            for item in fs::read_dir(path)? {
                let child = item?.path();
                // 원본 설정 로더의 디렉터리 모드는 JSON 파일만 읽는다.
                if child.is_dir()
                    || !json_only
                    || child
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                {
                    self.files(&child, json_only, inputs)?;
                }
            }
        } else {
            let bytes = fs::read(path)?;
            let content = if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                let raw = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
                match serde_json::from_slice::<Value>(raw) {
                    Ok(value) => serde_json::to_vec(&self.value(&value, false))?,
                    Err(_) => bytes,
                }
            } else {
                bytes
            };
            inputs.push(
                json!({"path":self.text(&path.to_string_lossy(),false),"sha256":sha(&content)}),
            );
        }
        Ok(())
    }
}
pub(crate) fn decode_fixture(bytes: &[u8]) -> Result<Value, Box<dyn Error>> {
    use std::io::Read;
    const LIMIT: u64 = 64 * 1024 * 1024;
    let mut decoded = Vec::new();
    let mut decoder = flate2::bufread::GzDecoder::new(bytes);
    decoder.by_ref().take(LIMIT + 1).read_to_end(&mut decoded)?;
    if decoded.len() as u64 > LIMIT {
        return Err("frozen baseline exceeds 64MiB decoded bound".into());
    }
    if !decoder.get_ref().is_empty() {
        return Err("trailing data after frozen gzip member".into());
    }
    Ok(serde_json::from_slice(&decoded)?)
}
pub fn legacy_expectation(family: &str, request: &Value) -> Result<Value, Box<dyn Error>> {
    if family.is_empty()
        || !family
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("invalid baseline family".into());
    }
    let case = Case::new(request);
    let identity = case.identity(family, request)?;
    let key = sha(&serde_json::to_vec(&identity)?);
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/legacy-baseline")
        .join(family)
        .join(format!("{key}.json.gz"));
    let file=fs::File::open(&path).map_err(|error|format!("Missing frozen baseline {family}/{key}: {error}. New cases require independently reviewed expectations."))?;
    use std::io::Read;
    const COMPRESSED_LIMIT: u64 = 64 * 1024 * 1024;
    let mut bytes = Vec::new();
    file.take(COMPRESSED_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > COMPRESSED_LIMIT {
        return Err("frozen compressed baseline exceeds 64MiB".into());
    }
    let fixture = decode_fixture(&bytes)?;
    if fixture["identity_sha256"] != key {
        return Err("baseline identity mismatch".into());
    }
    let expected = fixture
        .get("expected")
        .ok_or("missing frozen expected value")?;
    // 기대값을 변경하지 않고 이번 검사가 실제 사용한 자료만 선택적으로 집계한다.
    if let Some(directory) = std::env::var_os("D2BOT_BASELINE_COVERAGE_DIR") {
        if !Path::new(&directory).is_absolute() {
            return Err("baseline coverage directory must be an absolute task-owned path".into());
        }
        let owner = Path::new(&directory).join(family);
        fs::create_dir_all(&owner)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(owner.join(format!("{key}.seen")))
        {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(case.value(expected, true))
}
