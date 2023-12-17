use crate::inner::conf::traits::Evaluate;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum Filter {
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    Equals(String),
    NotEquals(String),
    #[serde(with = "serde_regex")]
    Regex(Regex),
}

impl PartialEq<Self> for Filter {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Filter::Contains(left), Filter::Contains(right)) => left == right,
            (Filter::StartsWith(left), Filter::StartsWith(right)) => left == right,
            (Filter::EndsWith(left), Filter::EndsWith(right)) => left == right,
            (Filter::Equals(left), Filter::Equals(right)) => left == right,
            (Filter::NotEquals(left), Filter::NotEquals(right)) => left == right,
            (Filter::Regex(left), Filter::Regex(right)) => left.as_str() == right.as_str(),
            _ => false,
        }
    }
}

impl Eq for Filter {}

impl Evaluate<&str, bool> for Filter {
    fn evaluate(&self, source: &str) -> bool {
        match self {
            Filter::Contains(value) => source.contains(value),
            Filter::StartsWith(value) => source.starts_with(value),
            Filter::EndsWith(value) => source.ends_with(value),
            Filter::Equals(value) => source == value,
            Filter::NotEquals(value) => source != value,
            Filter::Regex(value) => value.is_match(source),
        }
    }
}
