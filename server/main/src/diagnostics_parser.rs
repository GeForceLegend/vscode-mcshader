use std::path::PathBuf;

use hashbrown::HashMap;
use regex::Regex;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use url::Url;

use crate::opengl;

pub struct DiagnosticsParser {
    line_offset: u32,
    line_regex: Regex,
}

impl DiagnosticsParser {
    pub fn new(vendor_querier: &opengl::OpenGlContext) -> Self {
        let vendor = vendor_querier.vendor();
        let line_regex = match vendor.as_str() {
            "NVIDIA Corporation" => {
                Regex::new(r#"^(?P<filepath>\d+)\((?P<linenum>\d+)\) : (?P<severity>error|warning) [A-C]\d+: (?P<output>.+)"#).unwrap()
            }
            _ => Regex::new(
                r#"^(?P<severity>ERROR|WARNING): (?P<filepath>[^?<>*|"\n]+):(?P<linenum>\d+): (?:'.*' :|[a-z]+\(#\d+\)) +(?P<output>.+)$"#,
            )
            .unwrap(),
        };
        let line_offset = match vendor.as_str() {
            "AMD" | "ATI Technologies" | "ATI Technologies Inc." => 0,
            _ => 1,
        };
        DiagnosticsParser { line_offset, line_regex }
    }

    pub fn parse_diagnostics(&self, compile_log: String, file_list: HashMap<String, PathBuf>) -> HashMap<Url, Vec<Diagnostic>> {
        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();

        let default_path = file_list.get("0").unwrap();
        let default_path_name = default_path.to_str().unwrap();

        for log_line in compile_log.split_terminator('\n') {
            let diagnostic_capture = match self.line_regex.captures(log_line) {
                Some(captures) => captures,
                None => continue,
            };

            let mut msg = diagnostic_capture.name("output").unwrap().as_str().to_owned() + ", from file: ";
            msg += default_path_name;

            let line = match diagnostic_capture.name("linenum") {
                Some(c) => c.as_str().parse::<u32>().unwrap_or(0),
                None => 0,
            } - self.line_offset;

            let severity = match diagnostic_capture.name("severity") {
                Some(c) => match c.as_str().to_lowercase().as_str() {
                    "error" => DiagnosticSeverity::ERROR,
                    "warning" => DiagnosticSeverity::WARNING,
                    _ => DiagnosticSeverity::INFORMATION,
                },
                _ => DiagnosticSeverity::INFORMATION,
            };

            let file_path = match diagnostic_capture.name("filepath") {
                Some(index) => match file_list.get(index.as_str()) {
                    Some(file) => file,
                    None => default_path,
                },
                None => default_path,
            };
            let file_url = Url::from_file_path(file_path).unwrap();

            let diagnostic = Diagnostic {
                range: Range::new(Position::new(line, 0), Position::new(line, u32::MAX)),
                code: None,
                severity: Some(severity),
                source: Some("mcshader-glsl".into()),
                message: msg,
                related_information: None,
                tags: None,
                code_description: Option::None,
                data: Option::None,
            };

            match diagnostics.get_mut(&file_url) {
                Some(d) => d.push(diagnostic),
                None => {
                    diagnostics.insert(file_url, std::vec::from_elem(diagnostic, 1));
                }
            };
        }

        diagnostics
    }
}

pub trait DiagnosticsCollection {
    fn extend_diagnostics<T: IntoIterator<Item = (Url, Vec<Diagnostic>)>>(&mut self, iter: T);
}

impl DiagnosticsCollection for HashMap<Url, Vec<Diagnostic>> {
    fn extend_diagnostics<T: IntoIterator<Item = (Url, Vec<Diagnostic>)>>(&mut self, iter: T) {
        for diagnostic in iter {
            if let Some(diagnostics) = self.get_mut(&diagnostic.0) {
                diagnostics.extend(diagnostic.1);
            } else {
                self.insert(diagnostic.0, diagnostic.1);
            }
        }
    }
}
