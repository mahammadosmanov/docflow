//! Computed slot values (ADR 0058): a closed set of operations over other
//! slots. There is no expression language and no script: each operation is
//! Rust code, and a template only chooses one and names its inputs.

use std::{collections::BTreeMap, sync::LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

const MAX_INPUTS: usize = 10;
const MAX_DECIMALS: u8 = 6;
const MAX_SEPARATOR_CHARS: usize = 20;

static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-?[0-9]+([.,][0-9]+)?$").expect("static number regex"));

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Op {
    #[default]
    Sum,
    Difference,
    Offset,
    Round,
    Copy,
    Concat,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compute {
    pub op: Op,
    /// Keys of the slots the result is made from.
    #[serde(default)]
    pub inputs: Vec<String>,
    /// The fixed number `offset` adds (may be negative).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub by: String,
    /// Decimals of a number result; absent keeps the inputs' own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
    /// Text put between `concat` parts.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub separator: String,
}

fn decimals_of(text: &str) -> u8 {
    text.split_once(['.', ',']).map_or(0, |(_, tail)| {
        tail.len().min(usize::from(MAX_DECIMALS)) as u8
    })
}

fn parse(text: &str) -> Option<f64> {
    let text = text.trim();
    NUMBER
        .is_match(text)
        .then(|| text.replace(',', ".").parse().ok())
        .flatten()
}

fn format_number(value: f64, decimals: u8, comma: bool) -> String {
    let scale = 10f64.powi(i32::from(decimals));
    let rounded = (value * scale).round() / scale;
    // Avoid "-0" and "-0.0".
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let text = format!("{rounded:.*}", usize::from(decimals));
    if comma { text.replace('.', ",") } else { text }
}

impl Compute {
    /// The shape rules a saved template must satisfy.
    pub fn check(&self) -> Result<(), String> {
        let invalid = |reason: &str| Err(format!("A computed field is invalid: {reason}"));
        let count = self.inputs.len();
        if count == 0 || count > MAX_INPUTS {
            return invalid("it needs 1 to 10 inputs");
        }
        let exact = match self.op {
            Op::Difference => Some(2),
            Op::Offset | Op::Round | Op::Copy => Some(1),
            Op::Sum | Op::Concat => None,
        };
        if exact.is_some_and(|wanted| wanted != count) {
            return invalid("the number of inputs does not fit the operation");
        }
        if self
            .decimals
            .is_some_and(|decimals| decimals > MAX_DECIMALS)
        {
            return invalid("decimals are 0 to 6");
        }
        if (self.op == Op::Offset) != (!self.by.is_empty())
            || (self.op == Op::Offset && parse(&self.by).is_none())
        {
            return invalid("offset needs a number to add, and only offset has one");
        }
        if (self.op == Op::Round) && self.decimals.is_none() {
            return invalid("round needs decimals");
        }
        if self.separator.chars().count() > MAX_SEPARATOR_CHARS
            || (self.op != Op::Concat && !self.separator.is_empty())
            || self.separator.chars().any(char::is_control)
        {
            return invalid("the separator is not valid");
        }
        Ok(())
    }

    /// The result for these slot values (`label` names the field in errors).
    pub fn evaluate(
        &self,
        label: &str,
        values: &BTreeMap<String, String>,
    ) -> Result<String, String> {
        let get = |key: &String| values.get(key).map_or("", |text| text.trim());
        match self.op {
            Op::Copy => Ok(self.inputs.first().map_or("", get).to_string()),
            Op::Concat => Ok(self
                .inputs
                .iter()
                .map(get)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(&self.separator)),
            _ => self.number(label, values),
        }
    }

    fn number(&self, label: &str, values: &BTreeMap<String, String>) -> Result<String, String> {
        let mut numbers = Vec::new();
        let mut decimals = 0;
        let mut comma = false;
        for key in &self.inputs {
            let text = values.get(key).map_or("", |text| text.trim());
            if text.is_empty() {
                return Ok(String::new());
            }
            let number = parse(text)
                .ok_or_else(|| format!("{label} needs a number in {key}, not \"{text}\""))?;
            decimals = decimals.max(decimals_of(text));
            comma = comma || text.contains(',');
            numbers.push(number);
        }
        let result = match self.op {
            Op::Sum => numbers.iter().sum(),
            Op::Difference => numbers[0] - numbers[1],
            Op::Offset => {
                decimals = decimals.max(decimals_of(&self.by));
                numbers[0] + parse(&self.by).unwrap_or(0.0)
            }
            Op::Round | Op::Copy | Op::Concat => numbers[0],
        };
        Ok(format_number(
            result,
            self.decimals.unwrap_or(decimals),
            comma,
        ))
    }
}

#[cfg(test)]
mod tests;
