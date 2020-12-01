pub mod developers;
pub mod emails;
pub mod graphql_playground;
pub mod jumbotron;
pub mod navbar;
pub mod page;
pub mod profile;

pub mod forms;

/// Re-export everything in the static_pages module publicly.
pub mod static_pages;

pub use static_pages::*;

use std::collections::HashMap;
use serde_json::Value;
use serde::Serialize;
use handlebars::Handlebars;

/// A template that can be rendered using the handlebars template registry.
#[derive(Debug, Clone)]
pub struct Template {
    /// The file to use to render this template.
    pub handlebars_file: &'static str,

    /// The fields to render.
    fields: HashMap<String, Value>,
}

impl Template {
    /// Create a new template object with the path to the handlebars file from
    /// the templates directory.
    pub fn new(path: &'static str) -> Self {
        Self {
            handlebars_file: path,
            fields: HashMap::new(),
        }
    }

    /// Builder style method to add a field to this template instance.
    pub fn field(mut self, key: impl AsRef<String>, val: impl Serialize) -> Self {
        self.set_field(key, val);
        self
    }

    /// Setter method for fields on this template instance.
    pub fn set_field(&mut self, key: impl AsRef<String>, val: impl Serialize) {
        self.fields[key.as_ref()] = serde_json::to_value(val)
            .expect("Failed to serialize value.");
    }

    /// Render this template using a reference to the handlebars registry.
    pub fn render(&self, handlebars: &Handlebars) -> String {
        handlebars.render(self.handlebars_file, &self.fields)
            .expect("Could not render template.")
    }
}
