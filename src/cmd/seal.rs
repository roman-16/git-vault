use anyhow::Result;

use crate::exit::Code;
use crate::repo::Repo;
use crate::repo::session::Empty;

pub fn seal(allow_empty: bool) -> Result<Code> {
    let repo = Repo::discover()?;
    let sealed = repo.seal_worktree(if allow_empty {
        Empty::Allow
    } else {
        Empty::Refuse
    })?;

    if sealed.changed {
        println!(
            "Sealed {} secret{} into `.vault/data`.",
            sealed.secrets,
            if sealed.secrets == 1 { "" } else { "s" }
        );
    } else {
        println!("Already sealed.");
    }

    Ok(Code::Ok)
}
