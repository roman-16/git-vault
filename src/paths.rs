pub const ATTRIBUTES: &str = ".gitattributes";

pub const DATA: &str = ".vault/data";

pub const IGNORE: &str = ".gitignore";

pub const KEYS: &str = ".vault/keys";

pub const RECIPIENTS: &str = ".vault/recipients";

pub const VAULT_DIR: &str = ".vault";

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
