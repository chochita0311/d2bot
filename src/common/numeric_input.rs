//! 기존 정수 입력의 Unicode 문자·부호·자리수 의미를 손실 없이 유지한다.
mod unicode_numbers;
use serde_json::{Number, Value};
use unicode_numbers::{DECIMAL_STARTS, DIGIT_RANGES};
pub(crate) fn digit(character: char) -> bool {
    let code = character as u32;
    DIGIT_RANGES
        .iter()
        .any(|(low, high)| (*low..=*high).contains(&code))
}
pub(crate) fn decimal(character: char) -> Option<u32> {
    let code = character as u32;
    DECIMAL_STARTS
        .iter()
        .find_map(|start| code.checked_sub(*start).filter(|n| *n < 10))
}
pub(crate) fn strip(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}
pub(crate) fn integer(value: &str) -> Option<Number> {
    let value = strip(value);
    let (negative, value) = if let Some(v) = value.strip_prefix('-') {
        (true, v)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let mut digits = String::new();
    let mut was_digit = false;
    for character in value.chars() {
        if character == '_' {
            if !was_digit {
                return None;
            }
            was_digit = false;
        } else {
            let n = decimal(character)?;
            digits.push(char::from(b'0' + u8::try_from(n).ok()?));
            was_digit = true;
        }
    }
    if !was_digit {
        return None;
    }
    let digits = digits.trim_start_matches('0');
    let normalized = if digits.is_empty() {
        "0".into()
    } else if negative {
        format!("-{digits}")
    } else {
        digits.into()
    };
    normalized.parse().ok()
}
/// 표시 줄 수는 메모리를 미리 할당하지 않는다. 기계 주소 범위보다 큰 양수도 사실상 같은 상한이다.
pub(crate) fn visible_lines(value: &Value) -> Option<usize> {
    let number = match value {
        Value::Bool(_) => return Some(50),
        Value::String(v) => integer(v)?,
        Value::Number(v) => {
            let spelling = v.to_string();
            if spelling.contains(['.', 'e', 'E']) {
                let n = v.as_f64()?;
                if !n.is_finite() {
                    return None;
                }
                return Some((n.trunc().max(0.0) as usize).max(50));
            }
            v.clone()
        }
        _ => return None,
    };
    if number.to_string().starts_with('-') {
        Some(50)
    } else {
        Some(
            number
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .unwrap_or(usize::MAX)
                .max(50),
        )
    }
}
