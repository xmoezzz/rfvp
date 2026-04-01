use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use tower_lsp::lsp_types::Url;
use walkdir::WalkDir;

use crate::analysis::{analyze_document, DocumentAnalysis, SymbolDef};
use crate::config::LuaxConfig;
use crate::source::SourceMap;

#[derive(Debug, Clone)]
pub struct DocumentData {
    pub uri: Url,
    pub path: Option<PathBuf>,
    pub module_name: Option<String>,
    pub text: String,
    pub source: SourceMap,
    pub analysis: DocumentAnalysis,
    pub project_config: Option<LuaxConfig>,
    pub project_config_path: Option<PathBuf>,
    pub config_diagnostics: Vec<String>,
    pub version: i32,
}

#[derive(Debug, Default)]
pub struct WorkspaceState {
    pub root: Option<PathBuf>,
    pub docs: HashMap<Url, DocumentData>,
    pub modules: HashMap<String, Url>,
}

impl WorkspaceState {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            ..Self::default()
        }
    }

    pub fn scan_workspace(&mut self) -> Result<()> {
        let Some(root) = &self.root else {
            return Ok(());
        };
        let mut loaded = HashMap::new();
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !matches!(path.extension().and_then(|x| x.to_str()), Some("luax") | Some("lua")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(path) else {
                continue;
            };
            let Ok(uri) = Url::from_file_path(path) else {
                continue;
            };
            let version = self.docs.get(&uri).map(|d| d.version).unwrap_or(0);
            loaded.insert(uri.clone(), self.make_doc(uri, Some(path.to_path_buf()), text, version));
        }
        for (uri, doc) in loaded {
            self.docs.insert(uri, doc);
        }
        self.reanalyze_all();
        Ok(())
    }

    pub fn upsert_open_document(&mut self, uri: Url, text: String, version: i32) {
        let path = uri.to_file_path().ok();
        let doc = self.make_doc(uri.clone(), path, text, version);
        self.docs.insert(uri, doc);
        self.reanalyze_all();
    }

    pub fn remove_document(&mut self, uri: &Url) {
        self.docs.remove(uri);
        self.reanalyze_all();
    }

    fn make_doc(&self, uri: Url, path: Option<PathBuf>, text: String, version: i32) -> DocumentData {
        let module_name = self.compute_module_name(path.as_deref());
        let source = SourceMap::new(text.clone());
        let resolution = self.resolve_project_config(path.as_deref());
        let analysis = analyze_document(&text, module_name.clone(), resolution.config.as_ref());
        DocumentData {
            uri,
            path,
            module_name,
            text,
            source,
            analysis,
            project_config: resolution.config,
            project_config_path: resolution.path,
            config_diagnostics: resolution.diagnostics,
            version,
        }
    }

    fn compute_module_name(&self, path: Option<&Path>) -> Option<String> {
        let root = self.root.as_ref()?;
        let path = path?;
        let rel = path.strip_prefix(root).ok()?;
        let mut parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        if let Some(last) = parts.last_mut() {
            if let Some((stem, _)) = last.rsplit_once('.') {
                *last = stem.to_string();
            }
        }
        Some(parts.join("."))
    }

    fn resolve_project_config(&self, path: Option<&Path>) -> ProjectConfigResolution {
        let Some(path) = path else {
            return ProjectConfigResolution {
                config: None,
                path: None,
                diagnostics: Vec::new(),
            };
        };
        let Some(dir) = path.parent() else {
            return ProjectConfigResolution {
                config: None,
                path: None,
                diagnostics: vec!["missing project yaml in the same directory as this script".to_string()],
            };
        };

        let mut yaml_files = Vec::new();
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) => {
                return ProjectConfigResolution {
                    config: None,
                    path: None,
                    diagnostics: vec![format!("failed to inspect script directory for project yaml: {}", err)],
                }
            }
        };

        for entry in entries.filter_map(|entry| entry.ok()) {
            let candidate = entry.path();
            if !candidate.is_file() {
                continue;
            }
            if matches!(candidate.extension().and_then(|ext| ext.to_str()), Some("yaml") | Some("yml")) {
                yaml_files.push(candidate);
            }
        }
        yaml_files.sort();

        match yaml_files.len() {
            0 => ProjectConfigResolution {
                config: None,
                path: None,
                diagnostics: vec!["missing project yaml in the same directory as this script".to_string()],
            },
            1 => {
                let yaml_path = yaml_files.into_iter().next().unwrap();
                match LuaxConfig::load(&yaml_path) {
                    Ok(config) => ProjectConfigResolution {
                        config: Some(config),
                        path: Some(yaml_path),
                        diagnostics: Vec::new(),
                    },
                    Err(err) => ProjectConfigResolution {
                        config: None,
                        path: Some(yaml_path),
                        diagnostics: vec![format!("invalid project yaml: {}", err)],
                    },
                }
            }
            count => ProjectConfigResolution {
                config: None,
                path: None,
                diagnostics: vec![format!(
                    "expected exactly one project yaml in the script directory, found {}",
                    count
                )],
            },
        }
    }

    pub fn reanalyze_all(&mut self) {
        self.modules.clear();
        let docs_snapshot: Vec<(Url, Option<PathBuf>, String, i32)> = self
            .docs
            .iter()
            .map(|(uri, doc)| (uri.clone(), doc.path.clone(), doc.text.clone(), doc.version))
            .collect();
        self.docs.clear();
        for (uri, path, text, version) in docs_snapshot {
            let doc = self.make_doc(uri.clone(), path, text, version);
            if let Some(module) = &doc.module_name {
                self.modules.insert(module.clone(), uri.clone());
            }
            self.docs.insert(uri, doc);
        }
    }

    pub fn document(&self, uri: &Url) -> Option<&DocumentData> {
        self.docs.get(uri)
    }

    pub fn module_doc(&self, module_name: &str) -> Option<&DocumentData> {
        let uri = self.modules.get(module_name)?;
        self.docs.get(uri)
    }

    pub fn resolve_visible_symbol<'a>(
        &'a self,
        doc: &'a DocumentData,
        offset: usize,
        name: &str,
    ) -> Option<&'a SymbolDef> {
        doc.analysis
            .visible_defs(offset)
            .into_iter()
            .rev()
            .find(|def| def.name == name)
    }

    pub fn resolve_chain<'a>(
        &'a self,
        doc: &'a DocumentData,
        offset: usize,
        chain: &str,
    ) -> ChainResolution<'a> {
        let mut parts = chain.split('.');
        let Some(first) = parts.next() else {
            return ChainResolution::None;
        };
        if let Some(alias) = doc
            .analysis
            .aliases
            .iter()
            .find(|a| a.name == first && a.visible.start <= offset && offset <= a.visible.end)
        {
            let mut current_module = alias.module_name.clone();
            let mut last: Option<(&DocumentData, &SymbolDef)> = None;
            for part in parts {
                let Some(module_doc) = self.module_doc(&current_module) else {
                    return ChainResolution::Module(current_module);
                };
                if let Some(def) = module_doc.analysis.defs.iter().find(|d| d.parent.is_none() && d.name == part) {
                    last = Some((module_doc, def));
                    current_module = format!("{}.{}", current_module, part);
                } else {
                    return ChainResolution::Module(current_module);
                }
            }
            if let Some(pair) = last {
                return ChainResolution::CrossDocSymbol(pair);
            }
            return ChainResolution::Module(alias.module_name.clone());
        }
        let Some(mut current) = self.resolve_visible_symbol(doc, offset, first) else {
            return ChainResolution::None;
        };
        for part in parts {
            let Some(next_id) = current
                .children
                .iter()
                .find_map(|id| doc.analysis.defs.iter().find(|d| d.id == *id && d.name == part).map(|d| d.id))
            else {
                return ChainResolution::Symbol(current);
            };
            current = doc.analysis.defs.iter().find(|d| d.id == next_id).unwrap();
        }
        ChainResolution::Symbol(current)
    }
}

struct ProjectConfigResolution {
    config: Option<LuaxConfig>,
    path: Option<PathBuf>,
    diagnostics: Vec<String>,
}

pub enum ChainResolution<'a> {
    None,
    Symbol(&'a SymbolDef),
    Module(String),
    CrossDocSymbol((&'a DocumentData, &'a SymbolDef)),
}
