use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Thunderstore,
    Hexium,
    Local,
    Github,
}

impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Thunderstore => write!(f, "thunderstore"),
            Source::Hexium => write!(f, "hexium"),
            Source::Local => write!(f, "local"),
            Source::Github => write!(f, "github"),
        }
    }
}

impl From<Source> for String {
    fn from(source: Source) -> Self {
        source.to_string()
    }
}
