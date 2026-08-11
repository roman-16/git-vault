use std::path::PathBuf;

pub const ATTRIBUTES: &str = ".gitattributes";

pub const DATA: &str = ".vault/data";

pub const IGNORE: &str = ".gitignore";

pub const KEYS: &str = ".vault/keys";

pub const RECIPIENTS: &str = ".vault/recipients";

pub const VAULT_DIR: &str = ".vault";

#[cfg(windows)]
pub fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
        Some(plain) => PathBuf::from(plain),
        None => path,
    }
}

#[cfg(not(windows))]
pub const fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

pub fn is_vault_path(rel: &str) -> bool {
    rel == VAULT_DIR || rel.starts_with(".vault/")
}

pub fn is_never_sealed(rel: &str) -> bool {
    is_vault_path(rel)
        || rel == ATTRIBUTES
        || rel == IGNORE
        || rel == ".git"
        || rel.starts_with(".git/")
}

#[cfg(test)]
mod tests {
    use super::{is_never_sealed, is_vault_path};

    #[cfg(windows)]
    #[test]
    fn a_verbatim_prefix_is_dropped_because_git_refuses_one() {
        use std::path::PathBuf;

        use super::without_verbatim_prefix;

        assert_eq!(
            without_verbatim_prefix(PathBuf::from(r"\\?\C:\repo")),
            PathBuf::from(r"C:\repo")
        );
        assert_eq!(
            without_verbatim_prefix(PathBuf::from(r"C:\repo")),
            PathBuf::from(r"C:\repo")
        );
    }

    #[test]
    fn the_vault_directory_is_recognised() {
        assert!(is_vault_path(".vault"));
        assert!(is_vault_path(".vault/data"));
        assert!(is_vault_path(".vault/keys"));
        assert!(!is_vault_path(".vaulted"));
        assert!(!is_vault_path("secrets/.vault"));
    }

    #[test]
    fn the_files_that_make_the_repository_work_are_protected() {
        for path in [
            ".gitattributes",
            ".gitignore",
            ".git",
            ".git/config",
            ".vault/data",
            ".vault/keys",
        ] {
            assert!(is_never_sealed(path), "{path}");
        }

        assert!(!is_never_sealed("secrets/prod.env"));
    }
}
