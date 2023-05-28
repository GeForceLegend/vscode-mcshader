use std::path::{Path, PathBuf};

use hashbrown::HashMap;
use regex::Regex;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use url::Url;

use crate::{file::WorkspaceFile, opengl};

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

    pub fn parse_diagnostics(
        &self, workspace_files: &HashMap<PathBuf, WorkspaceFile>, compile_log: String, file_list: &HashMap<String, PathBuf>,
        shader_path: &Path,
    ) {
        let default_path = shader_path.to_str().unwrap();

        let mut diagnostics = file_list
            .iter()
            .map(|(index, path)| {
                let workspace_file = workspace_files.get(path).unwrap();
                let mut diagnostics = workspace_file.diagnostics().borrow_mut();
                diagnostics.insert(shader_path.to_path_buf(), vec![]);
                (index, diagnostics)
            })
            .collect::<Vec<(_, _)>>();

        let mut diagnostic_pointers = diagnostics
            .iter_mut()
            .map(|(index, diagnostics)| ((*index).clone(), diagnostics.get_mut(shader_path).unwrap()))
            .collect::<HashMap<_, _>>();

        compile_log
            .split_terminator('\n')
            .filter_map(|log_line| self.line_regex.captures(log_line))
            .for_each(|captures| {
                let mut msg = captures.name("output").unwrap().as_str().to_owned() + ", from file: ";
                msg += default_path;

                let line = match captures.name("linenum") {
                    Some(c) => c.as_str().parse::<u32>().unwrap_or(0),
                    None => 0,
                } - self.line_offset;

                let severity = match captures.name("severity") {
                    Some(c) => match c.as_str().to_lowercase().as_str() {
                        "error" => DiagnosticSeverity::ERROR,
                        "warning" => DiagnosticSeverity::WARNING,
                        _ => DiagnosticSeverity::INFORMATION,
                    },
                    _ => DiagnosticSeverity::INFORMATION,
                };

                let diagnostic = Diagnostic {
                    range: Range {
                        start: Position { line, character: 0 },
                        end: Position { line, character: u32::MAX },
                    },
                    severity: Some(severity),
                    source: Some("mcshader-glsl".to_owned()),
                    message: msg,
                    ..Diagnostic::default()
                };

                let index = captures.name("filepath").unwrap();
                if let Some(diagnostics) = diagnostic_pointers.get_mut(index.as_str()) {
                    diagnostics.push(diagnostic);
                }
            });
    }

    pub fn parse_temp_diagnostics(&self, compile_log: String, url: Url) -> HashMap<Url, Vec<Diagnostic>> {
        let diagnostics = compile_log
            .split_terminator('\n')
            .filter_map(|log_line| self.line_regex.captures(log_line))
            .filter(|captures| captures.name("filepath").unwrap().as_str() == "0")
            .map(|captures| {
                let msg = captures.name("output").unwrap().as_str().to_owned();

                let line = match captures.name("linenum") {
                    Some(c) => c.as_str().parse::<u32>().unwrap_or(0),
                    None => 0,
                } - self.line_offset;

                let severity = match captures.name("severity") {
                    Some(c) => match c.as_str().to_lowercase().as_str() {
                        "error" => DiagnosticSeverity::ERROR,
                        "warning" => DiagnosticSeverity::WARNING,
                        _ => DiagnosticSeverity::INFORMATION,
                    },
                    _ => DiagnosticSeverity::INFORMATION,
                };

                Diagnostic {
                    range: Range {
                        start: Position { line, character: 0 },
                        end: Position { line, character: u32::MAX },
                    },
                    severity: Some(severity),
                    source: Some("mcshader-glsl".to_owned()),
                    message: msg,
                    ..Diagnostic::default()
                }
            })
            .collect::<Vec<_>>();

        HashMap::from([(url, diagnostics)])
    }
}
