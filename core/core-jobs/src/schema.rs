use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// High-level category shown on the Tool Center home screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Ai,
    Images,
    Video,
    Audio,
    Pdf,
    Files,
    Convert,
}

impl ToolCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::Images => "images",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Pdf => "pdf",
            Self::Files => "files",
            Self::Convert => "convert",
        }
    }
}

/// Declares what inputs a tool accepts so the UI can validate and filter files automatically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputSpec {
    /// Allowed lowercase extensions without leading dot (empty = any file).
    pub accepted_extensions: Vec<String>,
    /// Minimum number of input paths required.
    pub min_files: usize,
    /// Optional maximum number of input paths (`None` = unlimited / batch-capable).
    pub max_files: Option<usize>,
    /// Whether folders can be passed as inputs (e.g. Duplicate Finder or Archive Create).
    pub allow_directories: bool,
}

impl ToolInputSpec {
    pub fn single_or_batch(exts: &[&str]) -> Self {
        Self {
            accepted_extensions: exts.iter().map(|s| s.to_ascii_lowercase()).collect(),
            min_files: 1,
            max_files: None,
            allow_directories: false,
        }
    }

    pub fn multi_minimum(exts: &[&str], min_files: usize) -> Self {
        Self {
            accepted_extensions: exts.iter().map(|s| s.to_ascii_lowercase()).collect(),
            min_files,
            max_files: None,
            allow_directories: false,
        }
    }

    pub fn single_file(exts: &[&str]) -> Self {
        Self {
            accepted_extensions: exts.iter().map(|s| s.to_ascii_lowercase()).collect(),
            min_files: 1,
            max_files: Some(1),
            allow_directories: false,
        }
    }
}

/// Choice item for a `Select` option field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectChoice {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

/// Condition controlling whether an option field is displayed in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibilityCondition {
    pub option_key: String,
    pub equals_value: Value,
}

/// Declarative UI control type for a tool option.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OptionKind {
    Boolean,
    Integer {
        min: Option<i64>,
        max: Option<i64>,
        step: Option<i64>,
        unit: Option<String>,
    },
    Number {
        min: Option<f64>,
        max: Option<f64>,
        step: Option<f64>,
        unit: Option<String>,
    },
    Text {
        placeholder: Option<String>,
        secret: bool,
    },
    Select {
        choices: Vec<SelectChoice>,
    },
    PositionGrid,
    FilePath {
        accepted_extensions: Vec<String>,
    },
}

/// A single plain-language option rendered automatically by the Flutter UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionField {
    pub key: String,
    /// Plain-language label (externalizable via `label_key`).
    pub label: String,
    pub label_key: String,
    /// Short, jargon-free explanation shown under the control.
    pub hint: String,
    pub hint_key: String,
    pub kind: OptionKind,
    pub default_value: Value,
    pub required: bool,
    pub visible_when: Option<VisibilityCondition>,
}

/// Complete self-contained metadata and schema for a registered `Tool`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub id: String,
    pub name: String,
    pub name_key: String,
    pub description: String,
    pub description_key: String,
    pub category: ToolCategory,
    pub icon: String,
    pub keywords: Vec<String>,
    pub input_spec: ToolInputSpec,
    pub options_schema: Vec<OptionField>,
    pub default_suffix: String,
}

/// Key-value map of option values supplied by the user, backed by schema defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolOptions {
    pub values: HashMap<String, Value>,
}

impl ToolOptions {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub fn from_json_str(raw: &str) -> Result<Self, serde_json::Error> {
        if raw.trim().is_empty() {
            return Ok(Self::new());
        }
        let values: HashMap<String, Value> = serde_json::from_str(raw)?;
        Ok(Self { values })
    }

    /// Merges schema defaults for any omitted keys.
    pub fn with_defaults(mut self, schema: &[OptionField]) -> Self {
        for field in schema {
            self.values
                .entry(field.key.clone())
                .or_insert_with(|| field.default_value.clone());
        }
        self
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.values.insert(key.into(), value.into());
    }

    pub fn get_str<'a>(&'a self, key: &str) -> Option<&'a str> {
        self.values.get(key).and_then(Value::as_str)
    }

    pub fn get_string_or(&self, key: &str, fallback: &str) -> String {
        self.get_str(key).unwrap_or(fallback).to_string()
    }

    pub fn get_bool(&self, key: &str, fallback: bool) -> bool {
        self.values
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(fallback)
    }

    pub fn get_i64(&self, key: &str, fallback: i64) -> i64 {
        self.values
            .get(key)
            .and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_f64().map(|f| f as i64))
                    .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
            })
            .unwrap_or(fallback)
    }

    pub fn get_f64(&self, key: &str, fallback: f64) -> f64 {
        self.values
            .get(key)
            .and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_i64().map(|i| i as f64))
                    .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
            })
            .unwrap_or(fallback)
    }
}
