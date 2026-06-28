mod json;
mod csv;
mod xlsx;
mod txt;
mod html;

use crate::error::RavenError;
use crate::types::{QueryResult, SearchResults};

pub use json::JsonReporter;
pub use csv::CsvReporter;
pub use xlsx::XlsxReporter;
pub use txt::TxtReporter;
pub use html::HtmlReporter;

pub trait Reporter: Send {
    fn write_search_start(&mut self, username: &str) -> Result<(), RavenError>;
    fn write_result(&mut self, result: &QueryResult) -> Result<(), RavenError>;
    fn write_search_complete(&mut self, results: &SearchResults) -> Result<(), RavenError>;
    fn finish(&mut self) -> Result<(), RavenError>;
}

pub struct Reporters {
    reporters: Vec<Box<dyn Reporter>>,
}

impl Reporters {
    pub fn new() -> Self {
        Reporters {
            reporters: Vec::new(),
        }
    }

    pub fn add<R: Reporter + 'static>(&mut self, reporter: R) {
        self.reporters.push(Box::new(reporter));
    }

    pub fn write_search_start(&mut self, username: &str) -> Result<(), RavenError> {
        for r in &mut self.reporters {
            r.write_search_start(username)?;
        }
        Ok(())
    }

    pub fn write_result(&mut self, result: &QueryResult) -> Result<(), RavenError> {
        for r in &mut self.reporters {
            r.write_result(result)?;
        }
        Ok(())
    }

    pub fn write_search_complete(&mut self, results: &SearchResults) -> Result<(), RavenError> {
        for r in &mut self.reporters {
            r.write_search_complete(results)?;
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), RavenError> {
        for r in &mut self.reporters {
            r.finish()?;
        }
        Ok(())
    }
}
