

/// Determines the MIME type of a file based on its extension
#[cfg(test)]
pub fn guess_mime_type(path: impl AsRef<Path>) -> Option<String> {
    let guess = from_path(path.as_ref());
    guess.first().map(|m| m.to_string())
}

/// Validates that a URI is properly formatted
#[cfg(test)]
pub fn validate_uri(uri: &str) -> Result<(), ResourceError> {
    lazy_static! {
        static ref URI_REGEX: Regex =
            Regex::new(r"^(https?://|file:///|git://|[a-zA-Z0-9-]+:)[^\s]*$").unwrap();
    }

    if URI_REGEX.is_match(uri) {
        Ok(())
    } else {
        Err(ResourceError::InvalidUri(uri.to_string()))
    }
}

/// Validates that a file path does not attempt path traversal
#[cfg(test)]
pub fn validate_path_safety(
    base_dir: impl AsRef<Path>,
    path: impl AsRef<Path>,
) -> Result<(), ResourceError> {
    let base_dir = base_dir.as_ref();
    let path = path.as_ref();

    // Convert to absolute paths
    let abs_base = base_dir
        .canonicalize()
        .map_err(|e| ResourceError::IoError(format!("Failed to canonicalize base path: {}", e)))?;

    let abs_path = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // Path might not exist yet, so we need to check manually
            let mut full_path = base_dir.to_path_buf();
            for component in path.components() {
                full_path.push(component);
            }
            full_path
        }
    };

    // Check that the path is within the base directory
    if !abs_path.starts_with(&abs_base) {
        return Err(ResourceError::SecurityViolation(format!(
            "Path traversal attempt: {:?} is outside {:?}",
            path, base_dir
        )));
    }

    Ok(())
}

/// Splits a URI into scheme and path
#[cfg(test)]
pub fn split_uri(uri: &str) -> Result<(String, String), ResourceError> {
    lazy_static! {
        static ref SCHEME_REGEX: Regex = Regex::new(r"^([a-zA-Z0-9-]+:)(.*)$").unwrap();
    }

    if let Some(captures) = SCHEME_REGEX.captures(uri) {
        let scheme = captures[1].to_string();
        let path = captures[2].to_string();
        Ok((scheme, path))
    } else {
        Err(ResourceError::InvalidUri(uri.to_string()))
    }
}

/// Encodes a binary payload as base64
#[cfg(test)]
pub fn encode_binary(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Decodes a base64 string into binary
#[cfg(test)]
pub fn decode_binary(data: &str) -> Result<Vec<u8>, ResourceError> {
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| ResourceError::DeserializationError(format!("Failed to decode base64: {}", e)))
}

/// Checks if a MIME type represents text content
#[cfg(test)]
pub fn is_text_mime_type(mime_type: &str) -> bool {
    mime_type.starts_with("text/")
        || mime_type == "application/json"
        || mime_type == "application/xml"
        || mime_type == "application/yaml"
        || mime_type == "application/toml"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_uri() {
        assert!(validate_uri("https://example.com").is_ok());
        assert!(validate_uri("file:///path/to/file").is_ok());
        assert!(validate_uri("git://github.com/user/repo").is_ok());
        assert!(validate_uri("custom:resource").is_ok());

        assert!(validate_uri("invalid uri").is_err());
        assert!(validate_uri("").is_err());
    }

    #[test]
    fn test_split_uri() {
        let (scheme, path) = split_uri("file:///path/to/file").unwrap();
        assert_eq!(scheme, "file:");
        assert_eq!(path, "//path/to/file");

        let (scheme, path) = split_uri("https://example.com").unwrap();
        assert_eq!(scheme, "https:");
        assert_eq!(path, "//example.com");

        assert!(split_uri("invalid").is_err());
    }

    #[test]
    fn test_is_text_mime_type() {
        assert!(is_text_mime_type("text/plain"));
        assert!(is_text_mime_type("text/markdown"));
        assert!(is_text_mime_type("application/json"));
        assert!(is_text_mime_type("application/yaml"));

        assert!(!is_text_mime_type("image/png"));
        assert!(!is_text_mime_type("application/octet-stream"));
    }
}
