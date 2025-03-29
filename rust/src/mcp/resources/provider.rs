use async_trait::async_trait;
use base64::Engine;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use crate::mcp::resources::error::ResourceError;
use crate::mcp::resources::models::*;

/// Provider for resource access
#[async_trait]
pub trait ResourceProvider: Send + Sync + std::fmt::Debug {
    /// Checks if this provider can handle a given URI
    fn can_handle_uri(&self, uri: &str) -> bool;

    /// Lists resources provided by this provider
    async fn list_resources(
        &self,
        cursor: Option<String>,
    ) -> Result<ResourceListResponse, ResourceError>;

    /// Reads a resource by URI
    async fn read_resource(&self, uri: &str) -> Result<ResourceReadResponse, ResourceError>;
}

/// Provider for file system resources
#[derive(Debug)]
pub struct FileSystemProvider {
    /// Base directory for files
    base_dir: PathBuf,
    /// MIME type mapping
    mime_types: HashMap<String, String>,
    /// File extensions to include
    include_extensions: Option<Vec<String>>,
    /// Number of resources per page
    page_size: usize,
}

impl FileSystemProvider {
    /// Creates a new file system provider
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
            mime_types: Self::default_mime_types(),
            include_extensions: None,
            page_size: 100,
        }
    }

    /// Sets file extensions to include
    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.include_extensions = Some(extensions);
        self
    }

    /// Sets the page size for listing resources
    pub fn with_page_size(mut self, page_size: usize) -> Self {
        self.page_size = page_size;
        self
    }

    /// Creates default MIME type mappings
    fn default_mime_types() -> HashMap<String, String> {
        let mut mime_types = HashMap::new();

        // Text files
        mime_types.insert("txt".to_string(), "text/plain".to_string());
        mime_types.insert("md".to_string(), "text/markdown".to_string());
        mime_types.insert("rst".to_string(), "text/x-rst".to_string());

        // Code files
        mime_types.insert("rs".to_string(), "text/x-rust".to_string());
        mime_types.insert("py".to_string(), "text/x-python".to_string());
        mime_types.insert("js".to_string(), "text/javascript".to_string());
        mime_types.insert("ts".to_string(), "text/typescript".to_string());
        mime_types.insert("html".to_string(), "text/html".to_string());
        mime_types.insert("css".to_string(), "text/css".to_string());
        mime_types.insert("json".to_string(), "application/json".to_string());
        mime_types.insert("yaml".to_string(), "application/yaml".to_string());
        mime_types.insert("yml".to_string(), "application/yaml".to_string());
        mime_types.insert("toml".to_string(), "application/toml".to_string());

        // Image files
        mime_types.insert("png".to_string(), "image/png".to_string());
        mime_types.insert("jpg".to_string(), "image/jpeg".to_string());
        mime_types.insert("jpeg".to_string(), "image/jpeg".to_string());
        mime_types.insert("gif".to_string(), "image/gif".to_string());
        mime_types.insert("svg".to_string(), "image/svg+xml".to_string());

        mime_types
    }

    /// Converts a file path to a resource URI
    fn path_to_uri(&self, path: &Path) -> String {
        let relative = path.strip_prefix(&self.base_dir).unwrap_or(path);
        format!("file:///{}", relative.to_string_lossy().replace('\\', "/"))
    }

    /// Converts a URI to a file path
    fn uri_to_path(&self, uri: &str) -> Result<PathBuf, ResourceError> {
        if !uri.starts_with("file:///") {
            return Err(ResourceError::InvalidUri(uri.to_string()));
        }

        let path_str = uri.trim_start_matches("file:///");
        let path = PathBuf::from(path_str);

        let full_path = self.base_dir.join(path);
        if !full_path.starts_with(&self.base_dir) {
            return Err(ResourceError::SecurityViolation(
                "Path traversal attempt detected".to_string(),
            ));
        }

        Ok(full_path)
    }

    /// Gets the MIME type for a file path
    fn get_mime_type(&self, path: &Path) -> Option<String> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| self.mime_types.get(ext))
            .cloned()
    }

    /// Checks if a file should be included based on extension
    fn should_include_file(&self, path: &Path) -> bool {
        if let Some(extensions) = &self.include_extensions {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                return extensions.iter().any(|e| e == ext);
            }
            return false;
        }
        true
    }

    /// Walks a directory and collects files
    fn walk_directory(&self, dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
        let mut result = Vec::new();

        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    result.extend(self.walk_directory(&path)?);
                } else if path.is_file() && self.should_include_file(&path) {
                    result.push(path);
                }
            }
        }

        Ok(result)
    }
}

#[async_trait]
impl ResourceProvider for FileSystemProvider {
    fn can_handle_uri(&self, uri: &str) -> bool {
        uri.starts_with("file:///")
    }

    async fn list_resources(
        &self,
        cursor: Option<String>,
    ) -> Result<ResourceListResponse, ResourceError> {
        let start_idx = cursor.and_then(|c| c.parse::<usize>().ok()).unwrap_or(0);

        let all_files = self
            .walk_directory(&self.base_dir)
            .map_err(|e| ResourceError::IoError(e.to_string()))?;

        let total = all_files.len();
        let end_idx = std::cmp::min(start_idx + self.page_size, total);

        let resources = all_files[start_idx..end_idx]
            .iter()
            .map(|path| {
                let metadata =
                    fs::metadata(path).map_err(|e| ResourceError::IoError(e.to_string()))?;

                let uri = self.path_to_uri(path);
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown")
                    .to_string();

                let mime_type = self.get_mime_type(path);

                Ok(Resource {
                    uri,
                    name,
                    description: None,
                    mime_type,
                    size: Some(metadata.len()),
                    metadata: std::collections::HashMap::new(),
                })
            })
            .collect::<Result<Vec<_>, ResourceError>>()?;

        let next_cursor = if end_idx < total {
            Some(end_idx.to_string())
        } else {
            None
        };

        Ok(ResourceListResponse {
            resources,
            next_cursor,
        })
    }

    async fn read_resource(&self, uri: &str) -> Result<ResourceReadResponse, ResourceError> {
        let path = self.uri_to_path(uri)?;

        if !path.exists() {
            return Err(ResourceError::ResourceNotFound(uri.to_string()));
        }

        let metadata = fs::metadata(&path).map_err(|e| ResourceError::IoError(e.to_string()))?;

        let mime_type = self.get_mime_type(&path);

        let contents = if metadata.len() > 10 * 1024 * 1024 {
            // File is too large
            return Err(ResourceError::ResourceTooLarge(uri.to_string()));
        } else if let Some(ref mime) = mime_type {
            if mime.starts_with("text/")
                || mime.ends_with("json")
                || mime.ends_with("yaml")
                || mime.ends_with("xml")
            {
                // Handle as text
                let mut file = File::open(&path)
                    .await
                    .map_err(|e| ResourceError::IoError(e.to_string()))?;

                let mut contents = String::new();
                file.read_to_string(&mut contents)
                    .await
                    .map_err(|e| ResourceError::IoError(e.to_string()))?;

                ResourceContents::Text(TextContent {
                    uri: uri.to_string(),
                    mime_type: mime_type.clone(),
                    text: contents,
                })
            } else {
                // Handle as binary
                let contents =
                    fs::read(&path).map_err(|e| ResourceError::IoError(e.to_string()))?;

                ResourceContents::Binary(BinaryContent {
                    uri: uri.to_string(),
                    mime_type: mime_type.clone(),
                    blob: base64::engine::general_purpose::STANDARD.encode(contents),
                })
            }
        } else {
            // No MIME type, treat as binary
            let contents = fs::read(&path).map_err(|e| ResourceError::IoError(e.to_string()))?;

            ResourceContents::Binary(BinaryContent {
                uri: uri.to_string(),
                mime_type: None,
                blob: base64::engine::general_purpose::STANDARD.encode(contents),
            })
        };

        Ok(ResourceReadResponse {
            contents: vec![contents],
        })
    }
}
